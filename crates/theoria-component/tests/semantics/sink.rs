use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use theoria_component::UpdateContext;
use theoria_event::EventTarget;

use super::support::{DrainOutcome, HandlesEvent, TestRuntime, receive};

fn deliver_all<E: 'static>(runtime: &mut TestRuntime<E>) {
    assert_eq!(runtime.run_until_idle(), DrainOutcome::Idle);
}

fn enqueue_root<E: 'static>(runtime: &TestRuntime<E>, event: E) -> bool {
    runtime.send(event)
}

#[derive(Clone, Copy)]
struct RoutedValue(u32);

enum LeftEvent {
    Routed(RoutedValue),
}

impl From<RoutedValue> for LeftEvent {
    fn from(value: RoutedValue) -> Self {
        Self::Routed(value)
    }
}

struct LeftRecorder(Rc<RefCell<Vec<String>>>);

impl HandlesEvent for LeftRecorder {
    type Event = LeftEvent;

    fn handle(&mut self, event: Self::Event, _context: &mut UpdateContext<'_>) {
        let LeftEvent::Routed(value) = event;
        self.0.borrow_mut().push(format!("left:{}", value.0));
    }
}

struct RightRecorder(Rc<RefCell<Vec<String>>>);

enum RightEvent {
    Routed(RoutedValue),
    Unmount,
}

impl From<RoutedValue> for RightEvent {
    fn from(value: RoutedValue) -> Self {
        Self::Routed(value)
    }
}

impl HandlesEvent for RightRecorder {
    type Event = RightEvent;

    fn handle(&mut self, event: Self::Event, context: &mut UpdateContext<'_>) {
        match event {
            RightEvent::Routed(value) => self.0.borrow_mut().push(format!("right:{}", value.0)),
            RightEvent::Unmount => context.unmount(),
        }
    }
}

struct ValueRouter {
    targets: Vec<EventTarget<RoutedValue>>,
}

impl HandlesEvent for ValueRouter {
    type Event = u32;

    fn handle(&mut self, event: Self::Event, context: &mut UpdateContext<'_>) {
        for target in &self.targets {
            assert!(context.send(target, RoutedValue(event)).is_ok());
        }
    }
}

#[test]
fn event_targets_store_heterogeneous_handlers_for_one_input_type() {
    let values = Rc::new(RefCell::new(Vec::new()));
    let mounted_values = Rc::clone(&values);
    let mut runtime = TestRuntime::new(move |context| {
        let left_values = Rc::clone(&mounted_values);
        let left = receive(context, LeftRecorder(left_values));
        let right = receive(context, RightRecorder(mounted_values));
        receive(
            context,
            ValueRouter {
                targets: vec![left.accepting(), right.accepting()],
            },
        )
    });

    assert!(enqueue_root(&runtime, 7_u32));
    deliver_all(&mut runtime);
    assert_eq!(&*values.borrow(), &["left:7", "right:7"]);
}

struct TargetProbe {
    target: EventTarget<RoutedValue>,
    result: Rc<Cell<Option<bool>>>,
}

impl HandlesEvent for TargetProbe {
    type Event = ();

    fn handle(&mut self, (): (), context: &mut UpdateContext<'_>) {
        self.result
            .set(Some(context.send(&self.target, RoutedValue(1)).is_ok()));
    }
}

#[test]
fn an_event_target_does_not_keep_an_unmounted_component_alive() {
    let result = Rc::new(Cell::new(None));
    let probe_result = Rc::clone(&result);
    let target_events = Rc::new(RefCell::new(Vec::new()));
    let mounted_target_events = Rc::clone(&target_events);
    let target_output = Rc::new(RefCell::new(None));
    let mounted_target = Rc::clone(&target_output);
    let mut runtime = TestRuntime::new(move |context| {
        let child_target = Rc::clone(&mounted_target);
        context.mount_with(move |context| {
            let target = receive(context, RightRecorder(mounted_target_events));
            child_target.replace(Some(target));
        });
        let erased = mounted_target
            .borrow()
            .as_ref()
            .expect("child construction exports its test input")
            .accepting();
        receive(
            context,
            TargetProbe {
                target: erased,
                result: probe_result,
            },
        )
    });
    let target = target_output.borrow_mut().take().expect("target");
    assert!(runtime.send_to(&target, RightEvent::Unmount));
    deliver_all(&mut runtime);
    assert!(enqueue_root(&runtime, ()));
    deliver_all(&mut runtime);
    assert_eq!(result.get(), Some(true));
    assert!(target_events.borrow().is_empty());
}
