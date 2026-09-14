use std::{
    cell::{Cell, RefCell},
    panic::{AssertUnwindSafe, catch_unwind},
    rc::Rc,
};

use theoria_component::{ComponentSink, UpdateContext};
use theoria_event::{Dispatch, DispatchOutcome, EventSink};

use super::support::{DrainOutcome, HandlesEvent, TestDriver, TestRuntime, receive};

fn deliver_all<E: 'static>(runtime: &mut TestRuntime<E>) {
    let outcome = runtime.run_until_idle();
    assert!(matches!(
        outcome,
        DrainOutcome::Idle | DrainOutcome::Finished
    ));
}

fn enqueue_root<E: 'static>(runtime: &TestRuntime<E>, event: E) -> bool {
    runtime.send(event)
}

struct Recorder(Rc<RefCell<Vec<u32>>>);

impl HandlesEvent for Recorder {
    type Event = u32;

    fn handle(&mut self, event: u32, _context: &mut UpdateContext<'_>) {
        self.0.borrow_mut().push(event);
    }
}

struct ParentWithState {
    value: theoria_reactive::State<i32>,
}

enum ParentEvent {
    Set(i32),
    Unmount,
}

impl HandlesEvent for ParentWithState {
    type Event = ParentEvent;

    fn handle(&mut self, event: ParentEvent, context: &mut UpdateContext<'_>) {
        match event {
            ParentEvent::Set(value) => {
                context.set(&self.value, value);
            }
            ParentEvent::Unmount => context.unmount(),
        }
    }
}

struct Passive;

impl HandlesEvent for Passive {
    type Event = ();

    fn handle(&mut self, (): (), _context: &mut UpdateContext<'_>) {}
}

#[test]
fn mount_context_mounts_a_child_that_can_read_its_owners_value() {
    let observed = Rc::new(RefCell::new(Vec::new()));
    let effect_observed = Rc::clone(&observed);
    let child_handle = Rc::new(RefCell::new(None));
    let exported_child = Rc::clone(&child_handle);
    let mut runtime = TestRuntime::new(|context| {
        let value = context.create_state(1);
        let child_value = value.value();
        let child_output = Rc::clone(&exported_child);
        context.mount_with(|context| {
            let effect = context.create_effect(move |context| {
                effect_observed
                    .borrow_mut()
                    .push(*context.read(&child_value));
            });
            context.sync(&effect);
            let child = receive(context, Passive);
            child_output.replace(Some(child));
        });
        receive(context, ParentWithState { value })
    });
    assert_eq!(runtime.run_until_idle(), DrainOutcome::Idle);

    enqueue_root(&runtime, ParentEvent::Set(4));
    deliver_all(&mut runtime);
    assert_eq!(&*observed.borrow(), &[1, 4]);

    let child = child_handle
        .borrow()
        .as_ref()
        .expect("child handle")
        .clone();
    enqueue_root(&runtime, ParentEvent::Unmount);
    deliver_all(&mut runtime);
    assert!(runtime.send_to(&child, ()));
    deliver_all(&mut runtime);
}

struct DynamicOwner {
    child_events: Rc<RefCell<Vec<u32>>>,
    exported_child: Rc<RefCell<Option<ComponentSink<u32>>>>,
}

impl HandlesEvent for DynamicOwner {
    type Event = DynamicOwnerEvent;

    fn handle(&mut self, event: DynamicOwnerEvent, context: &mut UpdateContext<'_>) {
        match event {
            DynamicOwnerEvent::Spawn(value) => {
                let child_events = Rc::clone(&self.child_events);
                let output = Rc::clone(&self.exported_child);
                assert!(context.mount_with(move |context| {
                    let child = receive(context, Recorder(child_events));
                    output.replace(Some(child));
                }));
                let child = self.exported_child.borrow();
                let child = child.as_ref().expect("mounted child");
                assert!(context.send(child, value).is_ok());
            }
            DynamicOwnerEvent::Unmount => context.unmount(),
        }
    }
}

enum DynamicOwnerEvent {
    Spawn(u32),
    Unmount,
}

#[test]
fn update_context_mounts_a_child_without_recursive_delivery() {
    let child_events = Rc::new(RefCell::new(Vec::new()));
    let exported_child = Rc::new(RefCell::new(None));
    let mut runtime = TestRuntime::new(|context| {
        receive(
            context,
            DynamicOwner {
                child_events: Rc::clone(&child_events),
                exported_child: Rc::clone(&exported_child),
            },
        )
    });
    enqueue_root(&runtime, DynamicOwnerEvent::Spawn(7));

    deliver_all(&mut runtime);
    assert_eq!(&*child_events.borrow(), &[7]);

    let child = exported_child
        .borrow()
        .as_ref()
        .expect("dynamically mounted child")
        .clone();
    enqueue_root(&runtime, DynamicOwnerEvent::Unmount);
    deliver_all(&mut runtime);
    assert!(runtime.send_to(&child, 8_u32));
    deliver_all(&mut runtime);
    assert_eq!(&*child_events.borrow(), &[7]);
}

struct SpawnThenRemove {
    child_events: Rc<RefCell<Vec<u32>>>,
    child_effect_runs: Rc<Cell<u32>>,
}

impl HandlesEvent for SpawnThenRemove {
    type Event = ();

    fn handle(&mut self, (): (), context: &mut UpdateContext<'_>) {
        let events = Rc::clone(&self.child_events);
        let effect_runs = Rc::clone(&self.child_effect_runs);
        let child_output = Rc::new(RefCell::new(None));
        let mounted_child = Rc::clone(&child_output);
        assert!(context.mount_with(move |context| {
            let effect =
                context.create_effect(move |_context| effect_runs.set(effect_runs.get() + 1));
            context.sync(&effect);
            let child = receive(context, Recorder(events));
            mounted_child.replace(Some(child));
        }));
        let child = child_output.borrow();
        let child = child.as_ref().expect("mounted child");
        assert!(context.send(child, 1_u32).is_ok());
        context.unmount();
    }
}

#[test]
fn a_child_mounted_by_an_unmounting_owner_is_removed_at_the_handler_boundary() {
    let child_events = Rc::new(RefCell::new(Vec::new()));
    let child_effect_runs = Rc::new(Cell::new(0));
    let mut runtime = TestRuntime::new(|context| {
        receive(
            context,
            SpawnThenRemove {
                child_events: Rc::clone(&child_events),
                child_effect_runs: Rc::clone(&child_effect_runs),
            },
        )
    });
    enqueue_root(&runtime, ());

    deliver_all(&mut runtime);
    assert!(child_events.borrow().is_empty());
    assert_eq!(child_effect_runs.get(), 0);
}

struct DropLog {
    name: &'static str,
    order: Rc<RefCell<Vec<&'static str>>>,
}

impl Drop for DropLog {
    fn drop(&mut self) {
        self.order.borrow_mut().push(self.name);
    }
}

#[test]
fn dropping_a_root_input_does_not_unmount_the_instance_owned_root() {
    let order = Rc::new(RefCell::new(Vec::new()));
    let parent_order = Rc::clone(&order);
    let runtime = TestRuntime::new(|context| {
        let child_order = Rc::clone(&order);
        context.mount_with(|context| {
            context.retain(DropLog {
                name: "child",
                order: child_order,
            });
        });
        context.retain(DropLog {
            name: "parent",
            order: parent_order,
        });
        context.receive(|(): (), _context| {})
    });

    let root_input = runtime.input().clone();
    drop(root_input);
    assert!(order.borrow().is_empty());
    drop(runtime);
    assert_eq!(&*order.borrow(), &["child", "parent"]);
}

#[test]
fn dropping_the_instance_destroys_owned_components_child_first() {
    let order = Rc::new(RefCell::new(Vec::new()));
    {
        let parent_order = Rc::clone(&order);
        let _runtime = TestDriver::new(|context| {
            let child_order = Rc::clone(&order);
            context.mount_with(|context| {
                context.retain(DropLog {
                    name: "child",
                    order: child_order,
                });
            });
            context.retain(DropLog {
                name: "parent",
                order: parent_order,
            });
        });
    }

    assert_eq!(&*order.borrow(), &["child", "parent"]);
}

#[test]
fn an_instance_without_a_root_input_still_owns_its_component_tree() {
    let order = Rc::new(RefCell::new(Vec::new()));
    let root_order = Rc::clone(&order);
    let runtime = TestDriver::new(move |context| {
        context.retain(DropLog {
            name: "root",
            order: root_order,
        });
    });

    assert!(order.borrow().is_empty());
    drop(runtime);
    assert_eq!(&*order.borrow(), &["root"]);
}

#[test]
fn dropping_the_instance_invalidates_a_root_while_its_input_remains() {
    let order = Rc::new(RefCell::new(Vec::new()));
    let target = {
        let component_order = Rc::clone(&order);
        let output = Rc::new(RefCell::new(None));
        let mounted_output = Rc::clone(&output);
        let runtime = TestDriver::new(move |context| {
            context.retain(DropLog {
                name: "root",
                order: component_order,
            });
            mounted_output.replace(Some(context.receive(|(): (), _context| {})));
        });
        let target = output.borrow_mut().take().expect("root input");
        drop(runtime);
        assert_eq!(&*order.borrow(), &["root"]);
        target
    };

    drop(target);
    assert_eq!(&*order.borrow(), &["root"]);
}

#[test]
fn an_instance_does_not_own_exported_typed_input_capabilities() {
    let events = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&events);
    let root_output = Rc::new(RefCell::new(None));
    let mounted_root = Rc::clone(&root_output);
    let mut runtime = TestDriver::new(move |context| {
        mounted_root.replace(Some(receive(context, Recorder(output))));
    });
    let root = root_output.borrow_mut().take().expect("root input");
    assert!(runtime.send(&root, 7_u32).is_ok());
    assert_eq!(runtime.run_until_idle(), Ok(DrainOutcome::Idle));
    assert_eq!(&*events.borrow(), &[7]);

    drop(runtime);
    assert_eq!(root.send(8_u32).run(), DispatchOutcome::SinkClosed);
}

#[test]
fn component_created_reactive_values_are_retained_until_component_destruction() {
    let order = Rc::new(RefCell::new(Vec::new()));
    {
        let component_order = Rc::clone(&order);
        let reactive_order = Rc::clone(&order);
        let runtime = TestDriver::new(|context| {
            let reactive_marker = DropLog {
                name: "reactive",
                order: reactive_order,
            };
            let _ = context.create_computed(move |_context| {
                let _ = &reactive_marker;
            });
            context.retain(DropLog {
                name: "component",
                order: component_order,
            });
        });
        assert!(order.borrow().is_empty());
        drop(runtime);
    }

    assert_eq!(&*order.borrow(), &["reactive", "component"]);
}

#[test]
fn a_panicking_mount_rolls_back_children_created_by_its_builder() {
    let child_events = Rc::new(RefCell::new(Vec::new()));
    let exported_child = Rc::new(RefCell::new(None));
    let builder_child = Rc::clone(&exported_child);
    let result = catch_unwind(AssertUnwindSafe(|| {
        let _runtime = TestDriver::new(|context| {
            let events = Rc::clone(&child_events);
            context.mount_with(move |context| {
                let child = receive(context, Recorder(events));
                builder_child.replace(Some(child));
            });
            panic!("requested mount panic");
        });
    }));

    assert!(result.is_err());
    let child = exported_child
        .borrow()
        .as_ref()
        .expect("child was constructed before the panic")
        .clone();
    assert_eq!(child.send(1_u32).run(), DispatchOutcome::SinkClosed);
}

struct ReactiveDropCounter(Rc<Cell<u32>>);

impl Drop for ReactiveDropCounter {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

struct FailedDynamicMount {
    drops: Rc<Cell<u32>>,
    effect_runs: Rc<Cell<u32>>,
}

impl HandlesEvent for FailedDynamicMount {
    type Event = ();

    fn handle(&mut self, (): (), context: &mut UpdateContext<'_>) {
        let drops = Rc::clone(&self.drops);
        let effect_runs = Rc::clone(&self.effect_runs);
        let failed = catch_unwind(AssertUnwindSafe(|| {
            context.mount_with(|cx| {
                let state = cx.create_state(ReactiveDropCounter(Rc::clone(&drops)));
                cx.retain(state);
                cx.mount_with(|cx| {
                    let state = cx.create_state(ReactiveDropCounter(drops));
                    cx.retain(state);
                    let effect = cx.create_effect(move |_| {
                        effect_runs.set(effect_runs.get() + 1);
                    });
                    cx.sync(&effect);
                });
                panic!("requested dynamic mount failure");
            });
        }));
        assert!(failed.is_err());
        // Cleanup finishes in the current mutation, even if the caller catches
        // construction failure and continues using the parent component.
        assert_eq!(self.drops.get(), 2);
        assert!(context.mount_with(|_| ()));
    }
}

#[test]
fn a_failed_dynamic_mount_releases_its_reactive_subtree_in_the_current_mutation() {
    let drops = Rc::new(Cell::new(0));
    let effect_runs = Rc::new(Cell::new(0));
    let mut instance = TestRuntime::new(|context| {
        receive(
            context,
            FailedDynamicMount {
                drops: Rc::clone(&drops),
                effect_runs: Rc::clone(&effect_runs),
            },
        )
    });
    assert!(instance.send(()));
    assert_eq!(instance.run_until_idle(), DrainOutcome::Idle);
    assert_eq!(drops.get(), 2);
    assert_eq!(effect_runs.get(), 0);
}

struct UnmountThenPanic {
    state: theoria_reactive::State<ReactiveDropCounter>,
    effect: theoria_reactive::Effect,
}

impl HandlesEvent for UnmountThenPanic {
    type Event = ();

    fn handle(&mut self, (): (), cx: &mut UpdateContext<'_>) {
        cx.unmount();
        cx.modify(&self.state, |value| {
            *value = ReactiveDropCounter(Rc::clone(&value.0));
        });
        cx.request_effect(&self.effect);
        panic!("requested failure after unmount");
    }
}

#[test]
fn externally_owned_reactive_values_outlive_unmount_and_handler_unwind() {
    let drops = Rc::new(Cell::new(0));
    let exported_value = Rc::new(RefCell::new(None));
    let mounted_value = Rc::clone(&exported_value);
    let mut instance = TestRuntime::new(|cx| {
        let state = cx.create_state(ReactiveDropCounter(Rc::clone(&drops)));
        let state_reader = state.value();
        mounted_value.replace(Some(cx.create_computed(move |read| {
            let _ = read.read(&state_reader);
        })));
        let effect = cx.create_effect(|_| panic!("unmounted Effect must not run"));
        receive(cx, UnmountThenPanic { state, effect })
    });
    assert!(instance.send(()));
    assert!(catch_unwind(AssertUnwindSafe(|| { instance.run_until_idle() })).is_err());
    // The replaced value is gone; the exported Computed retains its State
    // input independently of the Component.
    assert_eq!(drops.get(), 1);
    let exported_value = exported_value.borrow_mut().take().unwrap();
    assert!(exported_value.snapshot().is_ok());
    drop(exported_value);
    assert_eq!(drops.get(), 2);
    assert_eq!(instance.run_until_idle(), DrainOutcome::Finished);
}
