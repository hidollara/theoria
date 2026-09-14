use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use theoria_component::ComponentInstance;
use theoria_event::{
    BridgeProgress, Dispatch, DispatchOutcome, DispatchQueue, DispatchStatus, Dispatcher,
    EventTarget, OwningSink, SubmissionOutcome, bridge,
};

#[derive(Clone, Copy)]
struct TestImmediateDispatcher;

impl<D: Dispatch> Dispatcher<D> for TestImmediateDispatcher {
    fn submit(&self, dispatch: D) -> Result<SubmissionOutcome, D> {
        Ok(SubmissionOutcome::Executed(dispatch.run()))
    }
}

fn immediate_dispatcher() -> TestImmediateDispatcher {
    TestImmediateDispatcher
}

#[derive(Clone, Copy)]
struct RejectingDispatcher;

impl<D: Dispatch> Dispatcher<D> for RejectingDispatcher {
    fn submit(&self, dispatch: D) -> Result<SubmissionOutcome, D> {
        Err(dispatch)
    }
}

#[test]
fn handlers_do_not_run_until_root_construction_finishes() {
    let input = Rc::new(RefCell::new(None));
    let mounted_input = Rc::clone(&input);
    let runs = Rc::new(Cell::new(0));
    let handler_runs = Rc::clone(&runs);
    let instance = ComponentInstance::new(immediate_dispatcher(), move |context| {
        let input = context.receive(move |(), _context| {
            handler_runs.set(handler_runs.get() + 1);
        });

        assert!(EventTarget::from(&input).send_inline(()).is_ok());
        mounted_input.replace(Some(input));
    });
    let input = input
        .borrow_mut()
        .take()
        .expect("root construction creates its event input");

    assert_eq!(runs.get(), 0);
    assert_eq!(
        instance
            .send(&input, ())
            .expect("the immediate dispatcher accepts the event"),
        SubmissionOutcome::Executed(DispatchOutcome::Completed)
    );
    assert_eq!(runs.get(), 1);
}

#[test]
fn child_handlers_remain_inactive_until_root_construction_finishes() {
    let input = Rc::new(RefCell::new(None));
    let mounted_input = Rc::clone(&input);
    let runs = Rc::new(Cell::new(0));
    let child_runs = Rc::clone(&runs);
    let instance = ComponentInstance::new(immediate_dispatcher(), move |context| {
        context.mount_with(|child| {
            let input = child.receive(move |(), _context| {
                child_runs.set(child_runs.get() + 1);
            });
            assert!(EventTarget::from(&input).send_inline(()).is_ok());
            mounted_input.replace(Some(input));
        });

        let input = mounted_input.borrow();
        assert!(
            input
                .as_ref()
                .expect("child construction creates its event input")
                .send_inline(())
                .is_ok()
        );
    });
    let input = input
        .borrow_mut()
        .take()
        .expect("child construction creates its event input");

    assert_eq!(runs.get(), 0);
    assert_eq!(
        instance
            .send(&input, ())
            .expect("the immediate dispatcher accepts the event"),
        SubmissionOutcome::Executed(DispatchOutcome::Completed)
    );
    assert_eq!(runs.get(), 1);
}

#[test]
fn an_instance_can_use_a_caller_owned_dispatcher_and_flush_boundary() {
    let mut queue = DispatchQueue::new();
    let input = Rc::new(RefCell::new(None));
    let mounted_input = Rc::clone(&input);
    let observed = Rc::new(RefCell::new(Vec::new()));
    let effect_output = Rc::clone(&observed);
    let mut instance = ComponentInstance::new(queue.dispatcher(), move |context| {
        let state = context.create_state(0_u32);
        let effect_state = state.clone();
        let effect = context.create_effect(move |context| {
            effect_output
                .borrow_mut()
                .push(*context.read(&effect_state));
        });
        context.sync(&effect);
        mounted_input.replace(Some(context.receive(move |value, context| {
            context.set(&state, value);
        })));
    });
    let input = input
        .borrow_mut()
        .take()
        .expect("root construction creates its event input");

    instance.flush_effects().expect("valid initial Effect");
    assert_eq!(&*observed.borrow(), &[0]);
    assert_eq!(
        instance
            .send(&input, 7)
            .expect("the default dispatcher accepts the event"),
        SubmissionOutcome::Deferred
    );
    assert!(queue.run_next().is_some());
    assert_eq!(&*observed.borrow(), &[0]);

    instance.flush_effects().expect("valid updated Effect");
    assert_eq!(&*observed.borrow(), &[0, 7]);
    assert!(!instance.is_finished());
}

#[test]
fn inline_send_bypasses_the_default_dispatcher() {
    let mut queue = DispatchQueue::new();
    let input = Rc::new(RefCell::new(None));
    let mounted_input = Rc::clone(&input);
    let observed = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&observed);
    let instance = ComponentInstance::new(queue.dispatcher(), move |context| {
        mounted_input.replace(Some(context.receive(move |value, _context| {
            output.borrow_mut().push(value);
        })));
    });
    let input = input
        .borrow_mut()
        .take()
        .expect("root construction creates its event input");

    assert_eq!(
        instance
            .send(&input, "deferred")
            .expect("the default dispatcher accepts the event"),
        SubmissionOutcome::Deferred
    );
    assert!(observed.borrow().is_empty());

    assert_eq!(
        instance.send_inline(&input, "inline"),
        DispatchOutcome::Completed
    );
    assert_eq!(&*observed.borrow(), &["inline"]);

    assert!(queue.run_next().is_some());
    assert_eq!(&*observed.borrow(), &["inline", "deferred"]);
}

#[test]
fn an_inline_connection_handles_a_publication_on_the_publishing_stack() {
    let queue = DispatchQueue::new();
    let input = Rc::new(RefCell::new(None));
    let mounted_input = Rc::clone(&input);
    let observed = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&observed);
    let instance = ComponentInstance::new(queue.dispatcher(), move |context| {
        let (source, source_input) = context.event_source::<u32>();
        let sink = context.receive(move |value: u32, _context| {
            output.borrow_mut().push(value);
        });
        assert!(context.connect_inline(source, OwningSink::new(sink)));
        mounted_input.replace(Some(source_input));
    });
    let input = input
        .borrow_mut()
        .take()
        .expect("root construction creates its Source input");

    assert_eq!(
        instance.send_inline(input, 7_u32),
        DispatchOutcome::Completed
    );
    assert_eq!(&*observed.borrow(), &[7]);
    assert!(queue.is_empty());
}

#[test]
fn a_connection_accepts_a_dispatcher_without_erasing_it() {
    let queue = DispatchQueue::new();
    let input = Rc::new(RefCell::new(None));
    let mounted_input = Rc::clone(&input);
    let observed = Rc::new(Cell::new(0));
    let output = Rc::clone(&observed);
    let instance = ComponentInstance::new(queue.dispatcher(), move |context| {
        let (source, source_input) = context.event_source::<u32>();
        let sink = context.receive(move |value, _context| output.set(value));
        assert!(context.connect_via(TestImmediateDispatcher, source, OwningSink::new(sink)));
        mounted_input.replace(Some(source_input));
    });
    let input = input
        .borrow_mut()
        .take()
        .expect("root construction creates its Source input");

    assert_eq!(
        instance.send_inline(input, 7_u32),
        DispatchOutcome::Completed
    );
    assert_eq!(observed.get(), 7);
    assert!(queue.is_empty());
}

#[test]
fn an_instance_can_send_to_a_sink_with_its_own_dispatch_type() {
    let mut queue = DispatchQueue::new();
    let instance = ComponentInstance::new(queue.dispatcher(), |_| {});
    let (bridge, sink) = bridge();

    assert_eq!(
        instance
            .send(&sink, 7_u32)
            .expect("the default Dispatcher accepts the Bridge Dispatch"),
        SubmissionOutcome::Deferred
    );
    assert!(queue.run_next().is_some());
    assert_eq!(bridge.forward_next(), BridgeProgress::Forwarded);
}

#[test]
fn default_rejection_returns_an_executable_erased_dispatch() {
    let instance = ComponentInstance::new(RejectingDispatcher, |_| {});
    let (bridge, sink) = bridge();

    let dispatch = instance
        .send(&sink, 7_u32)
        .expect_err("the default Dispatcher rejects the Dispatch");
    assert_eq!(dispatch.run(), DispatchOutcome::Completed);
    assert_eq!(bridge.forward_next(), BridgeProgress::Forwarded);
}

#[test]
fn an_instance_can_track_a_sink_specific_dispatch() {
    let mut queue = DispatchQueue::new();
    let instance = ComponentInstance::new(queue.dispatcher(), |_| {});
    let (bridge, sink) = bridge();

    let handle = instance
        .send_tracked(&sink, 7_u32)
        .expect("the default Dispatcher accepts the tracked Bridge Dispatch");
    assert_eq!(handle.status(), DispatchStatus::Unstarted);
    assert!(queue.run_next().is_some());
    assert_eq!(
        handle.status(),
        DispatchStatus::Finished(DispatchOutcome::Completed)
    );
    assert_eq!(bridge.forward_next(), BridgeProgress::Forwarded);
}

#[test]
fn an_instance_can_send_through_default_and_explicit_dispatchers() {
    let mut default_queue = DispatchQueue::new();
    let mut priority_queue = DispatchQueue::new();
    let priority_dispatcher = priority_queue.dispatcher();
    let input = Rc::new(RefCell::new(None));
    let mounted_input = Rc::clone(&input);
    let observed = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&observed);
    let instance = ComponentInstance::new(default_queue.dispatcher(), move |context| {
        mounted_input.replace(Some(context.receive(move |value, _context| {
            output.borrow_mut().push(value);
        })));
    });
    let input = input
        .borrow_mut()
        .take()
        .expect("root construction creates its event input");

    assert_eq!(
        instance
            .send(&input, "default")
            .expect("the default dispatcher accepts the event"),
        SubmissionOutcome::Deferred
    );
    assert_eq!(
        instance
            .send_via(&priority_dispatcher, &input, "priority")
            .expect("the priority dispatcher accepts the event"),
        SubmissionOutcome::Deferred
    );

    assert!(priority_queue.run_next().is_some());
    assert_eq!(&*observed.borrow(), &["priority"]);
    assert!(default_queue.run_next().is_some());
    assert_eq!(&*observed.borrow(), &["priority", "default"]);
}

#[test]
fn one_source_can_connect_through_default_and_explicit_dispatchers() {
    let mut default_queue = DispatchQueue::new();
    let mut priority_queue = DispatchQueue::new();
    let priority_dispatcher = priority_queue.dispatcher();
    let mounted_priority_dispatcher = priority_dispatcher.clone();
    let input = Rc::new(RefCell::new(None));
    let mounted_input = Rc::clone(&input);
    let observed = Rc::new(RefCell::new(Vec::new()));
    let default_output = Rc::clone(&observed);
    let priority_output = Rc::clone(&observed);
    let instance = ComponentInstance::new(default_queue.dispatcher(), move |context| {
        let (source, source_input) = context.event_source::<u32>();
        let default_sink = context.receive(move |value, _context| {
            default_output.borrow_mut().push(("default", value));
        });
        let priority_sink = context.receive(move |value, _context| {
            priority_output.borrow_mut().push(("priority", value));
        });

        assert!(context.connect(source.clone(), OwningSink::new(default_sink)));
        assert!(context.connect_via(
            mounted_priority_dispatcher.clone(),
            source,
            OwningSink::new(priority_sink),
        ));
        mounted_input.replace(Some(source_input));
    });
    let input = input
        .borrow_mut()
        .take()
        .expect("root construction creates its Source input");

    assert_eq!(
        instance
            .send(&input, 7)
            .expect("the default dispatcher accepts the event"),
        SubmissionOutcome::Deferred
    );
    assert!(default_queue.run_next().is_some());

    assert!(priority_queue.run_next().is_some());
    assert_eq!(&*observed.borrow(), &[("priority", 7)]);
    assert!(default_queue.run_next().is_some());
    assert_eq!(&*observed.borrow(), &[("priority", 7), ("default", 7)]);
}

#[test]
fn unmount_deactivates_descendants_before_the_current_handler_returns() {
    let root_input = Rc::new(RefCell::new(None));
    let mounted_root_input = Rc::clone(&root_input);
    let child_runs = Rc::new(Cell::new(0));
    let child_output = Rc::clone(&child_runs);
    let instance = ComponentInstance::new(immediate_dispatcher(), move |context| {
        let mut child_input = None;
        context.mount_with(|context| {
            child_input = Some(context.receive(move |(), _context| {
                child_output.set(child_output.get() + 1);
            }));
        });
        let child_input = child_input.expect("child construction creates its event input");
        mounted_root_input.replace(Some(context.receive(move |(), context| {
            context.unmount();
            context
                .send(&child_input, ())
                .expect("immediate dispatch accepts the event");
        })));
    });
    let root_input = root_input
        .borrow_mut()
        .take()
        .expect("root construction creates its event input");

    instance
        .send(&root_input, ())
        .expect("immediate dispatch accepts the root event");

    assert_eq!(child_runs.get(), 0);
    assert!(instance.is_finished());
}
