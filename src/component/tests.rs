use std::{
    cell::{Cell, RefCell},
    panic::{AssertUnwindSafe, catch_unwind},
    rc::Rc,
};

use super::{
    Component, ComponentContext, ComponentHandle, ComponentRuntime, EventEmitter, EventSource,
    EventTarget, TurnBudget, TurnOutcome,
};

struct Counter {
    value: crate::reactive::State<i32>,
}

impl Component for Counter {
    type Event = i32;

    fn receive(&mut self, event: i32, context: &mut ComponentContext<'_>) {
        context.update(&self.value, |value| value + event);
    }
}

#[test]
fn events_update_component_owned_state_before_effects_run() {
    let runtime = ComponentRuntime::new();
    let observed = Rc::new(RefCell::new(Vec::new()));
    let effect_observed = Rc::clone(&observed);
    let counter = runtime.mount(|context| {
        let value = context.state(0);
        let observed_value = value.value();
        context.effect(move |context| {
            effect_observed
                .borrow_mut()
                .push(*context.read(&observed_value));
        });
        Counter { value }
    });
    runtime.flush_effects();

    assert!(runtime.enqueue(&counter, 2));
    assert!(runtime.enqueue(&counter, 3));
    assert_eq!(runtime.run_turn(TurnBudget::unlimited()), TurnOutcome::Idle);
    assert_eq!(&*observed.borrow(), &[0, 5]);
}

struct ParentWithState {
    value: crate::reactive::State<i32>,
    _child: ComponentHandle<Passive>,
}

impl Component for ParentWithState {
    type Event = i32;

    fn receive(&mut self, event: i32, context: &mut ComponentContext<'_>) {
        context.set(&self.value, event);
    }
}

struct Passive;

impl Component for Passive {
    type Event = ();

    fn receive(&mut self, (): (), _context: &mut ComponentContext<'_>) {}
}

#[test]
fn mount_context_mounts_a_child_that_can_read_its_owner_scope() {
    let runtime = ComponentRuntime::new();
    let observed = Rc::new(RefCell::new(Vec::new()));
    let effect_observed = Rc::clone(&observed);
    let child_handle = Rc::new(RefCell::new(None));
    let exported_child = Rc::clone(&child_handle);
    let parent = runtime.mount(|context| {
        let value = context.state(1);
        let child_value = value.value();
        let child = context.mount(|context| {
            context.effect(move |context| {
                effect_observed
                    .borrow_mut()
                    .push(*context.read(&child_value));
            });
            Passive
        });
        exported_child.replace(Some(child.clone()));
        ParentWithState {
            value,
            _child: child,
        }
    });
    runtime.flush_effects();

    runtime.enqueue(&parent, 4);
    assert_eq!(runtime.run_turn(TurnBudget::unlimited()), TurnOutcome::Idle);
    assert_eq!(&*observed.borrow(), &[1, 4]);

    let child = child_handle
        .borrow()
        .as_ref()
        .expect("child handle")
        .clone();
    assert!(runtime.unmount(&parent));
    assert!(!runtime.enqueue(&child, ()));
}

struct DynamicOwner {
    child_events: Rc<RefCell<Vec<u32>>>,
    exported_child: Rc<RefCell<Option<ComponentHandle<Recorder>>>>,
}

impl Component for DynamicOwner {
    type Event = u32;

    fn receive(&mut self, event: u32, context: &mut ComponentContext<'_>) {
        let child_events = Rc::clone(&self.child_events);
        let child = context.mount(move |_context| Recorder(child_events));
        self.exported_child.replace(Some(child.clone()));
        assert!(context.send(&child, event));
    }
}

#[test]
fn component_context_mounts_a_child_without_recursive_delivery() {
    let runtime = ComponentRuntime::new();
    let child_events = Rc::new(RefCell::new(Vec::new()));
    let exported_child = Rc::new(RefCell::new(None));
    let owner = runtime.mount(|_context| DynamicOwner {
        child_events: Rc::clone(&child_events),
        exported_child: Rc::clone(&exported_child),
    });
    runtime.enqueue(&owner, 7_u32);

    assert_eq!(runtime.run_turn(TurnBudget::unlimited()), TurnOutcome::Idle);
    assert_eq!(&*child_events.borrow(), &[7]);

    let child = exported_child
        .borrow()
        .as_ref()
        .expect("dynamically mounted child")
        .clone();
    assert!(runtime.unmount(&owner));
    assert!(!runtime.enqueue(&child, 8_u32));
}

struct SpawnThenRemove {
    child_events: Rc<RefCell<Vec<u32>>>,
    child_effect_runs: Rc<Cell<u32>>,
}

impl Component for SpawnThenRemove {
    type Event = ();

    fn receive(&mut self, (): (), context: &mut ComponentContext<'_>) {
        let events = Rc::clone(&self.child_events);
        let effect_runs = Rc::clone(&self.child_effect_runs);
        let child = context.mount(move |context| {
            context.effect(move |_context| effect_runs.set(effect_runs.get() + 1));
            Recorder(events)
        });
        assert!(context.send(&child, 1_u32));
        context.unmount_current();
    }
}

#[test]
fn a_child_mounted_by_an_unmounting_owner_is_removed_at_the_handler_boundary() {
    let runtime = ComponentRuntime::new();
    let child_events = Rc::new(RefCell::new(Vec::new()));
    let child_effect_runs = Rc::new(Cell::new(0));
    let owner = runtime.mount(|_context| SpawnThenRemove {
        child_events: Rc::clone(&child_events),
        child_effect_runs: Rc::clone(&child_effect_runs),
    });
    runtime.enqueue(&owner, ());

    assert_eq!(runtime.run_turn(TurnBudget::unlimited()), TurnOutcome::Idle);
    assert!(child_events.borrow().is_empty());
    assert_eq!(child_effect_runs.get(), 0);
}

struct DropLog {
    name: &'static str,
    order: Rc<RefCell<Vec<&'static str>>>,
}

impl Component for DropLog {
    type Event = ();

    fn receive(&mut self, (): (), _context: &mut ComponentContext<'_>) {}
}

impl Drop for DropLog {
    fn drop(&mut self) {
        self.order.borrow_mut().push(self.name);
    }
}

#[test]
fn owned_components_are_destroyed_child_first() {
    let runtime = ComponentRuntime::new();
    let order = Rc::new(RefCell::new(Vec::new()));
    let parent_order = Rc::clone(&order);
    let parent = runtime.mount(|context| {
        let child_order = Rc::clone(&order);
        context.mount(|_context| DropLog {
            name: "child",
            order: child_order,
        });
        DropLog {
            name: "parent",
            order: parent_order,
        }
    });

    assert!(runtime.unmount(&parent));
    assert_eq!(&*order.borrow(), &["child", "parent"]);
}

#[test]
fn dropping_the_runtime_also_destroys_owned_components_child_first() {
    let order = Rc::new(RefCell::new(Vec::new()));
    {
        let runtime = ComponentRuntime::new();
        let parent_order = Rc::clone(&order);
        runtime.mount(|context| {
            let child_order = Rc::clone(&order);
            context.mount(|_context| DropLog {
                name: "child",
                order: child_order,
            });
            DropLog {
                name: "parent",
                order: parent_order,
            }
        });
    }

    assert_eq!(&*order.borrow(), &["child", "parent"]);
}

#[test]
fn a_panicking_mount_rolls_back_children_created_by_its_builder() {
    let runtime = ComponentRuntime::new();
    let child_events = Rc::new(RefCell::new(Vec::new()));
    let exported_child = Rc::new(RefCell::new(None));
    let builder_child = Rc::clone(&exported_child);
    let result = catch_unwind(AssertUnwindSafe(|| {
        let _: ComponentHandle<Recorder> = runtime.mount(|context| {
            let events = Rc::clone(&child_events);
            let child = context.mount(move |_context| Recorder(events));
            builder_child.replace(Some(child));
            panic!("requested mount panic");
        });
    }));

    assert!(result.is_err());
    let child = exported_child
        .borrow()
        .as_ref()
        .expect("child was constructed before the panic")
        .clone();
    assert!(!runtime.enqueue(&child, 1_u32));
}

#[derive(Clone, Copy)]
struct Add(i32);

enum ReceiverEvent {
    Add(i32),
}

impl From<Add> for ReceiverEvent {
    fn from(value: Add) -> Self {
        Self::Add(value.0)
    }
}

struct Receiver(Rc<Cell<i32>>);

impl Component for Receiver {
    type Event = ReceiverEvent;

    fn receive(&mut self, event: ReceiverEvent, _context: &mut ComponentContext<'_>) {
        match event {
            ReceiverEvent::Add(value) => self.0.set(self.0.get() + value),
        }
    }
}

struct Forwarder {
    receiver: super::ComponentHandle<Receiver>,
}

impl Component for Forwarder {
    type Event = Add;

    fn receive(&mut self, event: Add, context: &mut ComponentContext<'_>) {
        assert!(context.send(&self.receiver, event));
    }
}

#[test]
fn generic_send_converts_events_and_queues_fifo_delivery() {
    let runtime = ComponentRuntime::new();
    let total = Rc::new(Cell::new(0));
    let receiver_total = Rc::clone(&total);
    let receiver = runtime.mount(move |_context| Receiver(receiver_total));
    let forwarder = runtime.mount(|_context| Forwarder {
        receiver: receiver.clone(),
    });

    runtime.enqueue(&forwarder, Add(2));
    runtime.enqueue(&receiver, ReceiverEvent::Add(3));
    assert_eq!(runtime.run_turn(TurnBudget::unlimited()), TurnOutcome::Idle);
    assert_eq!(total.get(), 5);
}

struct Recorder(Rc<RefCell<Vec<u32>>>);

impl Component for Recorder {
    type Event = u32;

    fn receive(&mut self, event: u32, _context: &mut ComponentContext<'_>) {
        self.0.borrow_mut().push(event);
    }
}

#[test]
fn budget_ends_a_turn_at_an_event_boundary() {
    let runtime = ComponentRuntime::new();
    let events = Rc::new(RefCell::new(Vec::new()));
    let component_events = Rc::clone(&events);
    let component = runtime.mount(move |_context| Recorder(component_events));
    for event in 0_u32..3 {
        runtime.enqueue(&component, event);
    }

    assert_eq!(
        runtime.run_turn(TurnBudget::events(2)),
        TurnOutcome::Pending
    );
    assert_eq!(&*events.borrow(), &[0, 1]);
    assert_eq!(runtime.run_turn(TurnBudget::events(2)), TurnOutcome::Idle);
    assert_eq!(&*events.borrow(), &[0, 1, 2]);
}

#[test]
fn effects_flush_at_each_budget_boundary() {
    let runtime = ComponentRuntime::new();
    let observed = Rc::new(RefCell::new(Vec::new()));
    let effect_observed = Rc::clone(&observed);
    let component = runtime.mount(|context| {
        let value = context.state(0);
        let observed_value = value.value();
        context.effect(move |context| {
            effect_observed
                .borrow_mut()
                .push(*context.read(&observed_value));
        });
        Counter { value }
    });
    runtime.flush_effects();
    runtime.enqueue(&component, 1);
    runtime.enqueue(&component, 1);

    assert_eq!(
        runtime.run_turn(TurnBudget::events(1)),
        TurnOutcome::Pending
    );
    assert_eq!(&*observed.borrow(), &[0, 1]);
    assert_eq!(runtime.run_turn(TurnBudget::events(1)), TurnOutcome::Idle);
    assert_eq!(&*observed.borrow(), &[0, 1, 2]);
}

struct SelfRemoving {
    inside_handler: Rc<Cell<bool>>,
    dropped_during_handler: Rc<Cell<bool>>,
}

impl Component for SelfRemoving {
    type Event = ();

    fn receive(&mut self, (): (), context: &mut ComponentContext<'_>) {
        self.inside_handler.set(true);
        context.unmount_current();
        assert!(self.inside_handler.get());
        self.inside_handler.set(false);
    }
}

impl Drop for SelfRemoving {
    fn drop(&mut self) {
        self.dropped_during_handler.set(self.inside_handler.get());
    }
}

#[test]
fn self_unmount_is_deferred_until_the_handler_returns() {
    let runtime = ComponentRuntime::new();
    let inside_handler = Rc::new(Cell::new(false));
    let dropped_during_handler = Rc::new(Cell::new(false));
    let effect_runs = Rc::new(Cell::new(0));
    let component_effect_runs = Rc::clone(&effect_runs);
    let component = runtime.mount(|context| {
        context.effect(move |_context| component_effect_runs.set(component_effect_runs.get() + 1));
        SelfRemoving {
            inside_handler: Rc::clone(&inside_handler),
            dropped_during_handler: Rc::clone(&dropped_during_handler),
        }
    });
    runtime.enqueue(&component, ());
    runtime.enqueue(&component, ());

    assert_eq!(runtime.run_turn(TurnBudget::unlimited()), TurnOutcome::Idle);
    assert!(!dropped_during_handler.get());
    assert!(!runtime.enqueue(&component, ()));
    assert_eq!(effect_runs.get(), 0);
}

struct RemovesOther {
    target: super::ComponentHandle<DropObserver>,
    inside_handler: Rc<Cell<bool>>,
}

impl Component for RemovesOther {
    type Event = ();

    fn receive(&mut self, (): (), context: &mut ComponentContext<'_>) {
        self.inside_handler.set(true);
        assert!(context.unmount(&self.target));
        self.inside_handler.set(false);
    }
}

struct DropObserver {
    events: Rc<RefCell<Vec<u32>>>,
    inside_other_handler: Rc<Cell<bool>>,
    dropped_during_handler: Rc<Cell<bool>>,
}

impl Component for DropObserver {
    type Event = u32;

    fn receive(&mut self, event: u32, _context: &mut ComponentContext<'_>) {
        self.events.borrow_mut().push(event);
    }
}

impl Drop for DropObserver {
    fn drop(&mut self) {
        self.dropped_during_handler
            .set(self.inside_other_handler.get());
    }
}

#[test]
fn unmounting_another_component_waits_for_the_current_handler_boundary() {
    let runtime = ComponentRuntime::new();
    let inside_handler = Rc::new(Cell::new(false));
    let dropped_during_handler = Rc::new(Cell::new(false));
    let events = Rc::new(RefCell::new(Vec::new()));
    let target = runtime.mount(|_context| DropObserver {
        events: Rc::clone(&events),
        inside_other_handler: Rc::clone(&inside_handler),
        dropped_during_handler: Rc::clone(&dropped_during_handler),
    });
    let remover = runtime.mount(|_context| RemovesOther {
        target: target.clone(),
        inside_handler: Rc::clone(&inside_handler),
    });
    runtime.enqueue(&remover, ());
    runtime.enqueue(&target, 1_u32);

    assert_eq!(runtime.run_turn(TurnBudget::unlimited()), TurnOutcome::Idle);
    assert!(!dropped_during_handler.get());
    assert!(events.borrow().is_empty());
}

#[test]
fn queued_events_for_an_unmounted_component_are_discarded() {
    let runtime = ComponentRuntime::new();
    let events = Rc::new(RefCell::new(Vec::new()));
    let component_events = Rc::clone(&events);
    let component = runtime.mount(move |_context| Recorder(component_events));
    runtime.enqueue(&component, 1_u32);

    assert!(runtime.unmount(&component));
    assert_eq!(runtime.run_turn(TurnBudget::unlimited()), TurnOutcome::Idle);
    assert!(events.borrow().is_empty());
}

#[test]
fn stale_handles_cannot_target_a_reused_slot() {
    let runtime = ComponentRuntime::new();
    let stale_events = Rc::new(RefCell::new(Vec::new()));
    let first_events = Rc::clone(&stale_events);
    let stale = runtime.mount(move |_context| Recorder(first_events));
    assert!(runtime.unmount(&stale));

    let replacement_events = Rc::new(RefCell::new(Vec::new()));
    let mounted_events = Rc::clone(&replacement_events);
    let replacement = runtime.mount(move |_context| Recorder(mounted_events));
    assert!(!runtime.enqueue(&stale, 1_u32));
    assert!(runtime.enqueue(&replacement, 2_u32));
    let _ = runtime.run_turn(TurnBudget::unlimited());

    assert!(stale_events.borrow().is_empty());
    assert_eq!(&*replacement_events.borrow(), &[2]);
}

struct PanicsOnce {
    panic: bool,
    runs: Rc<Cell<u32>>,
}

impl Component for PanicsOnce {
    type Event = ();

    fn receive(&mut self, (): (), _context: &mut ComponentContext<'_>) {
        self.runs.set(self.runs.get() + 1);
        if self.panic {
            self.panic = false;
            panic!("requested component panic");
        }
    }
}

#[test]
fn a_panicking_handler_does_not_poison_runtime_or_component_slot() {
    let runtime = ComponentRuntime::new();
    let runs = Rc::new(Cell::new(0));
    let component_runs = Rc::clone(&runs);
    let component = runtime.mount(move |_context| PanicsOnce {
        panic: true,
        runs: component_runs,
    });
    runtime.enqueue(&component, ());

    assert!(
        catch_unwind(AssertUnwindSafe(|| {
            let _ = runtime.run_turn(TurnBudget::unlimited());
        }))
        .is_err()
    );
    assert!(runtime.enqueue(&component, ()));
    assert_eq!(runtime.run_turn(TurnBudget::unlimited()), TurnOutcome::Idle);
    assert_eq!(runs.get(), 2);
}

#[test]
fn handles_cannot_cross_runtime_boundaries() {
    let first = ComponentRuntime::new();
    let second = ComponentRuntime::new();
    let events = Rc::new(RefCell::new(Vec::new()));
    let component = first.mount(move |_context| Recorder(events));

    assert!(catch_unwind(AssertUnwindSafe(|| second.enqueue(&component, 1_u32))).is_err());
}

struct MoveOnlyEvent(String);

struct EventPublisher {
    first: EventEmitter<MoveOnlyEvent>,
    second: EventEmitter<MoveOnlyEvent>,
    subscriber_log: Rc<RefCell<Vec<String>>>,
}

impl Component for EventPublisher {
    type Event = (bool, MoveOnlyEvent);

    fn receive(&mut self, (use_second, event): Self::Event, context: &mut ComponentContext<'_>) {
        assert!(
            self.subscriber_log.borrow().is_empty(),
            "publishing must not invoke subscribers recursively"
        );
        let emitter = if use_second {
            &self.second
        } else {
            &self.first
        };
        assert!(context.emit(emitter, event));
    }
}

struct StringRecorder {
    values: Rc<RefCell<Vec<String>>>,
}

impl Component for StringRecorder {
    type Event = String;

    fn receive(&mut self, event: String, _context: &mut ComponentContext<'_>) {
        self.values.borrow_mut().push(event);
    }
}

#[test]
fn one_source_supports_repeated_subscriptions_and_cloneable_emitters() {
    let runtime = ComponentRuntime::new();
    let values = Rc::new(RefCell::new(Vec::new()));
    let publisher_values = Rc::clone(&values);
    let mounted_values = Rc::clone(&values);
    let publisher = runtime.mount(move |context| {
        let first_values = Rc::clone(&mounted_values);
        let first = context.mount(move |_context| StringRecorder {
            values: first_values,
        });
        let second_values = Rc::clone(&mounted_values);
        let second = context.mount(move |_context| StringRecorder {
            values: second_values,
        });
        let (source, emitter) = context.event::<MoveOnlyEvent>();
        assert!(context.subscribe_with(&source, &first, |event| format!("first:{}", event.0)));
        assert!(context.subscribe_with(&source, &second, |event| format!("second:{}", event.0)));
        EventPublisher {
            first: emitter.clone(),
            second: emitter,
            subscriber_log: publisher_values,
        }
    });

    assert!(runtime.enqueue(&publisher, (false, MoveOnlyEvent("one".into()))));
    assert_eq!(runtime.run_turn(TurnBudget::unlimited()), TurnOutcome::Idle);
    assert_eq!(&*values.borrow(), &["first:one", "second:one"]);

    values.borrow_mut().clear();
    assert!(runtime.enqueue(&publisher, (true, MoveOnlyEvent("two".into()))));
    assert_eq!(runtime.run_turn(TurnBudget::unlimited()), TurnOutcome::Idle);
    assert_eq!(&*values.borrow(), &["first:two", "second:two"]);
}

enum SelectedSource {
    Left(u32),
    Right(u32),
}

struct SeparateSources {
    left: EventEmitter<u32>,
    right: EventEmitter<u32>,
}

impl Component for SeparateSources {
    type Event = SelectedSource;

    fn receive(&mut self, event: Self::Event, context: &mut ComponentContext<'_>) {
        match event {
            SelectedSource::Left(value) => assert!(context.emit(&self.left, value)),
            SelectedSource::Right(value) => assert!(context.emit(&self.right, value)),
        }
    }
}

#[test]
fn sources_with_the_same_event_type_keep_distinct_identities() {
    let runtime = ComponentRuntime::new();
    let values = Rc::new(RefCell::new(Vec::new()));
    let mounted_values = Rc::clone(&values);
    let publisher = runtime.mount(move |context| {
        let target = context.mount(move |_context| StringRecorder {
            values: mounted_values,
        });
        let (left_source, left) = context.event::<u32>();
        let (right_source, right) = context.event::<u32>();
        assert!(context.subscribe_with(&left_source, &target, |value| format!("left:{value}")));
        assert!(context.subscribe_with(&right_source, &target, |value| format!("right:{value}")));
        SeparateSources { left, right }
    });

    runtime.enqueue(&publisher, SelectedSource::Right(2));
    runtime.enqueue(&publisher, SelectedSource::Left(1));
    assert_eq!(runtime.run_turn(TurnBudget::unlimited()), TurnOutcome::Idle);
    assert_eq!(&*values.borrow(), &["right:2", "left:1"]);
}

struct U32Publisher(EventEmitter<u32>);

impl Component for U32Publisher {
    type Event = u32;

    fn receive(&mut self, event: u32, context: &mut ComponentContext<'_>) {
        assert!(context.emit(&self.0, event));
    }
}

struct RecordedU32(u32);

impl From<&u32> for RecordedU32 {
    fn from(value: &u32) -> Self {
        Self(*value)
    }
}

struct U32Recorder(Rc<RefCell<Vec<u32>>>);

impl Component for U32Recorder {
    type Event = RecordedU32;

    fn receive(&mut self, event: RecordedU32, _context: &mut ComponentContext<'_>) {
        self.0.borrow_mut().push(event.0);
    }
}

#[test]
fn unmounting_a_subscription_owner_disconnects_it() {
    let runtime = ComponentRuntime::new();
    let exported_source = Rc::new(RefCell::new(None::<EventSource<u32>>));
    let source_output = Rc::clone(&exported_source);
    let publisher = runtime.mount(move |context| {
        let (source, emitter) = context.event();
        source_output.replace(Some(source));
        U32Publisher(emitter)
    });
    let values = Rc::new(RefCell::new(Vec::new()));
    let target_values = Rc::clone(&values);
    let target = runtime.mount(move |_context| U32Recorder(target_values));
    let subscription_owner = runtime.mount(|context| {
        assert!(context.subscribe(
            exported_source.borrow().as_ref().expect("exported source"),
            &target,
        ));
        Passive
    });

    assert!(runtime.unmount(&subscription_owner));
    assert!(runtime.enqueue(&publisher, 7_u32));
    assert_eq!(runtime.run_turn(TurnBudget::unlimited()), TurnOutcome::Idle);
    assert!(values.borrow().is_empty());
}

struct EmitProbe {
    emitter: EventEmitter<u32>,
    result: Rc<Cell<Option<bool>>>,
}

impl Component for EmitProbe {
    type Event = u32;

    fn receive(&mut self, event: u32, context: &mut ComponentContext<'_>) {
        self.result.set(Some(context.emit(&self.emitter, event)));
    }
}

#[test]
fn emitter_clones_do_not_keep_their_source_alive() {
    let runtime = ComponentRuntime::new();
    let exported_emitter = Rc::new(RefCell::new(None::<EventEmitter<u32>>));
    let emitter_output = Rc::clone(&exported_emitter);
    let source_owner = runtime.mount(move |context| {
        let (_source, emitter) = context.event();
        emitter_output.replace(Some(emitter));
        Passive
    });
    let result = Rc::new(Cell::new(None));
    let probe_result = Rc::clone(&result);
    let probe = runtime.mount(move |_context| EmitProbe {
        emitter: exported_emitter
            .borrow_mut()
            .take()
            .expect("exported emitter"),
        result: probe_result,
    });

    assert!(runtime.unmount(&source_owner));
    assert!(runtime.enqueue(&probe, 1_u32));
    assert_eq!(runtime.run_turn(TurnBudget::unlimited()), TurnOutcome::Idle);
    assert_eq!(result.get(), Some(false));
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

impl Component for LeftRecorder {
    type Event = LeftEvent;

    fn receive(&mut self, event: Self::Event, _context: &mut ComponentContext<'_>) {
        let LeftEvent::Routed(value) = event;
        self.0.borrow_mut().push(format!("left:{}", value.0));
    }
}

struct RightRecorder(Rc<RefCell<Vec<String>>>);

impl Component for RightRecorder {
    type Event = RoutedValue;

    fn receive(&mut self, event: Self::Event, _context: &mut ComponentContext<'_>) {
        self.0.borrow_mut().push(format!("right:{}", event.0));
    }
}

struct ValueRouter {
    targets: Vec<EventTarget<RoutedValue>>,
}

impl Component for ValueRouter {
    type Event = u32;

    fn receive(&mut self, event: Self::Event, context: &mut ComponentContext<'_>) {
        for target in &self.targets {
            assert!(context.send_to(target, RoutedValue(event)));
        }
    }
}

#[test]
fn event_targets_store_heterogeneous_components_for_one_event_type() {
    let runtime = ComponentRuntime::new();
    let values = Rc::new(RefCell::new(Vec::new()));
    let mounted_values = Rc::clone(&values);
    let router = runtime.mount(move |context| {
        let left_values = Rc::clone(&mounted_values);
        let left = context.mount(move |_context| LeftRecorder(left_values));
        let right = context.mount(move |_context| RightRecorder(mounted_values));
        ValueRouter {
            targets: vec![left.event_target(), right.event_target()],
        }
    });

    assert!(runtime.enqueue(&router, 7_u32));
    assert_eq!(runtime.run_turn(TurnBudget::unlimited()), TurnOutcome::Idle);
    assert_eq!(&*values.borrow(), &["left:7", "right:7"]);
}

struct TargetProbe {
    target: EventTarget<RoutedValue>,
    result: Rc<Cell<Option<bool>>>,
}

impl Component for TargetProbe {
    type Event = ();

    fn receive(&mut self, (): (), context: &mut ComponentContext<'_>) {
        self.result
            .set(Some(context.send_to(&self.target, RoutedValue(1))));
    }
}

#[test]
fn event_target_does_not_keep_an_unmounted_component_alive() {
    let runtime = ComponentRuntime::new();
    let target = runtime.mount(|_context| RightRecorder(Rc::new(RefCell::new(Vec::new()))));
    let erased = target.event_target();
    assert!(runtime.unmount(&target));

    let result = Rc::new(Cell::new(None));
    let probe_result = Rc::clone(&result);
    let probe = runtime.mount(move |_context| TargetProbe {
        target: erased,
        result: probe_result,
    });
    assert!(runtime.enqueue(&probe, ()));
    assert_eq!(runtime.run_turn(TurnBudget::unlimited()), TurnOutcome::Idle);
    assert_eq!(result.get(), Some(false));
}
