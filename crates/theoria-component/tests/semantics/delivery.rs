use std::{
    cell::{Cell, RefCell},
    panic::{AssertUnwindSafe, catch_unwind},
    rc::Rc,
};

use theoria_component::UpdateContext;
use theoria_event::EventTarget;

use super::support::{DrainOutcome, HandlesEvent, TestRuntime, receive};

fn deliver_all<E: 'static>(runtime: &mut TestRuntime<E>) {
    assert!(matches!(
        runtime.run_until_idle(),
        DrainOutcome::Idle | DrainOutcome::Finished
    ));
}

fn deliver_exactly<E: 'static>(runtime: &mut TestRuntime<E>, deliveries: usize) {
    assert_eq!(runtime.run_events(deliveries), DrainOutcome::Paused);
}

fn enqueue_root<E: 'static>(runtime: &TestRuntime<E>, event: E) -> bool {
    runtime.send(event)
}

struct Counter {
    value: theoria_reactive::State<i32>,
}

impl HandlesEvent for Counter {
    type Event = i32;

    fn handle(&mut self, event: i32, context: &mut UpdateContext<'_>) {
        context.modify(&self.value, |value| *value += event);
    }
}

#[derive(Clone, Copy)]
struct Add(i32);

enum AddEvent {
    Add(i32),
}

impl From<Add> for AddEvent {
    fn from(value: Add) -> Self {
        Self::Add(value.0)
    }
}

struct AddHandler(Rc<Cell<i32>>);

impl HandlesEvent for AddHandler {
    type Event = AddEvent;

    fn handle(&mut self, event: AddEvent, _context: &mut UpdateContext<'_>) {
        match event {
            AddEvent::Add(value) => self.0.set(self.0.get() + value),
        }
    }
}

struct Forwarder {
    target: EventTarget<AddEvent>,
}

impl HandlesEvent for Forwarder {
    type Event = Add;

    fn handle(&mut self, event: Add, context: &mut UpdateContext<'_>) {
        assert!(context.send(&self.target, event.into()).is_ok());
    }
}

#[test]
fn a_sink_submits_an_explicitly_converted_event_to_fifo_delivery() {
    let total = Rc::new(Cell::new(0));
    let handler_total = Rc::clone(&total);
    let mut runtime = TestRuntime::new(move |context| {
        let target = receive(context, AddHandler(handler_total));
        receive(context, Forwarder { target })
    });

    enqueue_root(&runtime, Add(2));
    enqueue_root(&runtime, Add(3));
    deliver_all(&mut runtime);
    assert_eq!(total.get(), 5);
}

struct Recorder(Rc<RefCell<Vec<u32>>>);

impl HandlesEvent for Recorder {
    type Event = u32;

    fn handle(&mut self, event: u32, _context: &mut UpdateContext<'_>) {
        self.0.borrow_mut().push(event);
    }
}

enum RemovableRecorderEvent {
    Record(u32),
    Unmount,
}

struct RemovableRecorder(Rc<RefCell<Vec<u32>>>);

impl HandlesEvent for RemovableRecorder {
    type Event = RemovableRecorderEvent;

    fn handle(&mut self, event: Self::Event, context: &mut UpdateContext<'_>) {
        match event {
            RemovableRecorderEvent::Record(value) => self.0.borrow_mut().push(value),
            RemovableRecorderEvent::Unmount => context.unmount(),
        }
    }
}

#[test]
fn the_caller_can_pause_between_event_deliveries() {
    let events = Rc::new(RefCell::new(Vec::new()));
    let component_events = Rc::clone(&events);
    let mut runtime = TestRuntime::new(move |context| receive(context, Recorder(component_events)));
    for event in 0_u32..3 {
        enqueue_root(&runtime, event);
    }

    deliver_exactly(&mut runtime, 2);
    assert_eq!(&*events.borrow(), &[0, 1]);
    deliver_exactly(&mut runtime, 1);
    assert_eq!(&*events.borrow(), &[0, 1, 2]);
    assert_eq!(runtime.run_until_idle(), DrainOutcome::Idle);
}

#[test]
fn each_caller_selected_flush_boundary_runs_effects() {
    let observed = Rc::new(RefCell::new(Vec::new()));
    let effect_observed = Rc::clone(&observed);
    let mut runtime = TestRuntime::new(|context| {
        let value = context.create_state(0);
        let observed_value = value.value();
        let effect = context.create_effect(move |context| {
            effect_observed
                .borrow_mut()
                .push(*context.read(&observed_value));
        });
        context.sync(&effect);
        receive(context, Counter { value })
    });
    assert_eq!(runtime.run_until_idle(), DrainOutcome::Idle);
    enqueue_root(&runtime, 1);
    enqueue_root(&runtime, 1);

    deliver_exactly(&mut runtime, 1);
    assert_eq!(&*observed.borrow(), &[0, 1]);
    assert_eq!(runtime.run_until_idle(), DrainOutcome::Idle);
    assert_eq!(&*observed.borrow(), &[0, 1, 2]);
}

struct SelfRemoving {
    inside_handler: Rc<Cell<bool>>,
    dropped: Rc<Cell<bool>>,
    dropped_during_handler: Rc<Cell<bool>>,
    rejected_mount_builder_ran: Rc<Cell<bool>>,
}

impl HandlesEvent for SelfRemoving {
    type Event = ();

    fn handle(&mut self, (): (), context: &mut UpdateContext<'_>) {
        self.inside_handler.set(true);
        context.unmount();
        let builder_ran = Rc::clone(&self.rejected_mount_builder_ran);
        assert!(!context.mount_with(move |_context| {
            builder_ran.set(true);
        }));
        assert!(self.inside_handler.get());
        self.inside_handler.set(false);
    }
}

impl Drop for SelfRemoving {
    fn drop(&mut self) {
        self.dropped.set(true);
        self.dropped_during_handler.set(self.inside_handler.get());
    }
}

#[test]
fn self_unmount_is_deferred_until_the_handler_returns() {
    let inside_handler = Rc::new(Cell::new(false));
    let dropped = Rc::new(Cell::new(false));
    let dropped_during_handler = Rc::new(Cell::new(false));
    let rejected_mount_builder_ran = Rc::new(Cell::new(false));
    let effect_runs = Rc::new(Cell::new(0));
    let component_effect_runs = Rc::clone(&effect_runs);
    let mut runtime = TestRuntime::new(|context| {
        let effect = context.create_effect(move |_context| {
            component_effect_runs.set(component_effect_runs.get() + 1)
        });
        context.sync(&effect);
        receive(
            context,
            SelfRemoving {
                inside_handler: Rc::clone(&inside_handler),
                dropped: Rc::clone(&dropped),
                dropped_during_handler: Rc::clone(&dropped_during_handler),
                rejected_mount_builder_ran: Rc::clone(&rejected_mount_builder_ran),
            },
        )
    });
    enqueue_root(&runtime, ());
    enqueue_root(&runtime, ());

    assert_eq!(runtime.run_events(1), DrainOutcome::Finished);
    assert!(dropped.get());
    assert!(!dropped_during_handler.get());
    assert!(!rejected_mount_builder_ran.get());
    assert!(enqueue_root(&runtime, ()));
    assert_eq!(effect_runs.get(), 0);
    assert_eq!(runtime.run_until_idle(), DrainOutcome::Finished);
}

#[test]
fn queued_events_for_an_unmounted_component_are_cancelled() {
    let events = Rc::new(RefCell::new(Vec::new()));
    let component_events = Rc::clone(&events);
    let mut runtime =
        TestRuntime::new(move |context| receive(context, RemovableRecorder(component_events)));
    let component_target = runtime.event_target().clone();
    enqueue_root(&runtime, RemovableRecorderEvent::Unmount);
    enqueue_root(&runtime, RemovableRecorderEvent::Record(1));

    deliver_all(&mut runtime);
    assert!(events.borrow().is_empty());
    assert!(runtime.send_to(&component_target, RemovableRecorderEvent::Record(2)));
    deliver_all(&mut runtime);
    assert!(events.borrow().is_empty());
}

struct PanicsOnce {
    panic: bool,
    runs: Rc<Cell<u32>>,
}

impl HandlesEvent for PanicsOnce {
    type Event = ();

    fn handle(&mut self, (): (), _context: &mut UpdateContext<'_>) {
        self.runs.set(self.runs.get() + 1);
        if self.panic {
            self.panic = false;
            panic!("requested component panic");
        }
    }
}

#[test]
fn a_panicking_handler_is_restored_for_later_delivery() {
    let runs = Rc::new(Cell::new(0));
    let component_runs = Rc::clone(&runs);
    let mut runtime = TestRuntime::new(move |context| {
        receive(
            context,
            PanicsOnce {
                panic: true,
                runs: component_runs,
            },
        )
    });
    enqueue_root(&runtime, ());

    assert!(
        catch_unwind(AssertUnwindSafe(|| {
            deliver_all(&mut runtime);
        }))
        .is_err()
    );
    assert!(enqueue_root(&runtime, ()));
    deliver_all(&mut runtime);
    assert_eq!(runs.get(), 2);
}
