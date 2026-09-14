use std::{
    cell::{Cell, RefCell},
    panic::{AssertUnwindSafe, catch_unwind},
    rc::Rc,
};

use theoria_component::{ComponentSink, UpdateContext};
use theoria_event::EventSink;

use super::support::{DrainOutcome, HandlesEvent, TestRuntime, receive};

fn deliver_all<E: 'static>(runtime: &mut TestRuntime<E>) {
    assert_eq!(runtime.run_until_idle(), DrainOutcome::Idle);
}

fn enqueue_root<E: 'static>(runtime: &TestRuntime<E>, event: E) -> bool {
    runtime.send(event)
}

#[test]
fn recursively_sending_to_the_same_component_sink_panics() {
    let recurse = Rc::new(Cell::new(true));
    let nested_recurse = Rc::clone(&recurse);
    let sink = Rc::new(RefCell::new(None::<ComponentSink<()>>));
    let nested_sink = Rc::clone(&sink);
    let calls = Rc::new(Cell::new(0));
    let callback_calls = Rc::clone(&calls);
    let mut runtime = TestRuntime::new(move |context| {
        let input = context.receive(move |(): (), context| {
            callback_calls.set(callback_calls.get() + 1);
            if nested_recurse.replace(false) {
                let input = nested_sink
                    .borrow()
                    .as_ref()
                    .expect("the Component input has been installed")
                    .clone();
                let _ = context.send_inline(input, ());
            }
        });
        sink.replace(Some(input.clone()));
        input
    });

    assert!(enqueue_root(&runtime, ()));
    assert!(
        catch_unwind(AssertUnwindSafe(|| runtime.run_until_idle())).is_err(),
        "recursive delivery must not alias the callback mutably"
    );
    assert!(enqueue_root(&runtime, ()));
    deliver_all(&mut runtime);
    assert_eq!(calls.get(), 2);
}

#[derive(Clone, Copy)]
struct RoutedValue(u32);

type RoutedTarget = dyn Fn(RoutedValue, &mut UpdateContext<'_>) -> bool;

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
    targets: Vec<Box<RoutedTarget>>,
}

impl HandlesEvent for ValueRouter {
    type Event = u32;

    fn handle(&mut self, event: Self::Event, context: &mut UpdateContext<'_>) {
        for target in &self.targets {
            assert!(target(RoutedValue(event), context));
        }
    }
}

#[test]
fn heterogeneous_sink_operations_can_be_erased_at_their_storage_boundary() {
    let values = Rc::new(RefCell::new(Vec::new()));
    let mounted_values = Rc::clone(&values);
    let mut runtime = TestRuntime::new(move |context| {
        let left_values = Rc::clone(&mounted_values);
        let left = receive(context, LeftRecorder(left_values)).accepting();
        let right = receive(context, RightRecorder(mounted_values)).accepting();
        receive(
            context,
            ValueRouter {
                targets: vec![
                    Box::new(move |event, context| context.send(&left, event).is_ok()),
                    Box::new(move |event, context| context.send(&right, event).is_ok()),
                ],
            },
        )
    });

    assert!(enqueue_root(&runtime, 7_u32));
    deliver_all(&mut runtime);
    assert_eq!(&*values.borrow(), &["left:7", "right:7"]);
}

struct TargetProbe {
    target: Box<RoutedTarget>,
    result: Rc<Cell<Option<bool>>>,
}

impl HandlesEvent for TargetProbe {
    type Event = ();

    fn handle(&mut self, (): (), context: &mut UpdateContext<'_>) {
        self.result
            .set(Some((self.target)(RoutedValue(1), context)));
    }
}

#[test]
fn an_adapted_event_sink_does_not_keep_an_unmounted_component_alive() {
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
        let target = mounted_target
            .borrow()
            .as_ref()
            .expect("child construction exports its test input")
            .clone()
            .accepting();
        receive(
            context,
            TargetProbe {
                target: Box::new(move |event, context| context.send(&target, event).is_ok()),
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
