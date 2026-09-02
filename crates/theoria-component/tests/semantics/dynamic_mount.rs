use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use theoria_component::{Component, MountContext, UpdateContext};
use theoria_event::{EventSink, EventSource};

use super::support::{DrainOutcome, HandlesEvent, TestRuntime, receive};

fn deliver_all<E: 'static>(runtime: &mut TestRuntime<E>) {
    assert_eq!(runtime.run_until_idle(), DrainOutcome::Idle);
}

fn enqueue_root<E: 'static>(runtime: &TestRuntime<E>, event: E) -> bool {
    runtime.send(event)
}

struct OwnedU32Publisher {
    input: EventSink<u32>,
}

impl HandlesEvent for OwnedU32Publisher {
    type Event = u32;

    fn handle(&mut self, event: u32, context: &mut UpdateContext<'_>) {
        assert!(context.send(&self.input, event).is_ok());
    }
}

enum DynamicHandlerEvent {
    Install,
    Publish(u32),
}

struct DynamicHandlerOwner {
    source: EventSource<u32>,
    input: EventSink<u32>,
    values: Rc<RefCell<Vec<u32>>>,
}

struct U32Subscriber {
    source: EventSource<u32>,
    values: Rc<RefCell<Vec<u32>>>,
}

impl Component for U32Subscriber {
    fn mount(self, context: &mut MountContext<'_>) {
        let target = context.receive(move |value, _context| self.values.borrow_mut().push(value));
        assert!(context.connect_map(&self.source, &target, |value| *value));
    }
}

impl HandlesEvent for DynamicHandlerOwner {
    type Event = DynamicHandlerEvent;

    fn handle(&mut self, event: Self::Event, context: &mut UpdateContext<'_>) {
        match event {
            DynamicHandlerEvent::Install => {
                assert!(context.mount(U32Subscriber {
                    source: self.source.clone(),
                    values: Rc::clone(&self.values),
                }));
            }
            DynamicHandlerEvent::Publish(value) => {
                assert!(context.send(&self.input, value).is_ok());
                assert!(self.values.borrow().is_empty());
            }
        }
    }
}

#[test]
fn a_component_can_mount_a_handler_during_an_event() {
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let mut runtime = TestRuntime::new(move |context| {
        let (source, input) = context.event_source();
        receive(
            context,
            DynamicHandlerOwner {
                source,
                input,
                values: output,
            },
        )
    });

    assert!(enqueue_root(&runtime, DynamicHandlerEvent::Install));
    assert!(enqueue_root(&runtime, DynamicHandlerEvent::Publish(7)));
    deliver_all(&mut runtime);
    assert_eq!(&*values.borrow(), &[7]);
}

#[test]
fn a_dynamically_mounted_component_owns_reactive_nodes_created_by_its_builder() {
    let observed = Rc::new(Cell::new(0_u32));
    let effect_output = Rc::clone(&observed);
    let mut runtime = TestRuntime::new(move |context| {
        let (source, input) = context.event_source::<u32>();
        context.mount_with(move |handler_context| {
            let count = handler_context.create_state(0_u32);
            let count_value = count.value();
            let effect = handler_context.create_effect(move |context| {
                effect_output.set(*context.read(&count_value));
            });
            handler_context.sync(&effect);
            let handler = handler_context.receive(move |value, context: &mut UpdateContext<'_>| {
                context.modify(&count, |count| *count += value);
            });
            assert!(handler_context.connect_map(&source, &handler, |value| *value));
        });
        receive(context, OwnedU32Publisher { input })
    });
    assert_eq!(runtime.run_until_idle(), DrainOutcome::Idle);
    assert_eq!(observed.get(), 0);

    assert!(enqueue_root(&runtime, 7_u32));
    deliver_all(&mut runtime);
    assert_eq!(observed.get(), 7);
}
