use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use theoria_component::ComponentInstance;
use theoria_event::{DispatchOutcome, Dispatcher, EventSink, FifoDispatcher, SubmissionOutcome};

#[test]
fn handlers_do_not_run_until_root_construction_finishes() {
    let input = Rc::new(RefCell::new(None));
    let mounted_input = Rc::clone(&input);
    let runs = Rc::new(Cell::new(0));
    let handler_runs = Rc::clone(&runs);
    let instance = ComponentInstance::new(Dispatcher::immediate(), move |context| {
        let input = context.receive(move |(), _context| {
            handler_runs.set(handler_runs.get() + 1);
        });

        assert!(EventSink::from(&input).send(()).is_ok());
        mounted_input.replace(Some(input));
    });
    let input = input
        .borrow_mut()
        .take()
        .expect("root construction creates its event input");

    assert_eq!(runs.get(), 0);
    assert_eq!(
        instance.send(&input, ()),
        Ok(SubmissionOutcome::Completed(DispatchOutcome::Delivered))
    );
    assert_eq!(runs.get(), 1);
}

#[test]
fn child_handlers_remain_inactive_until_root_construction_finishes() {
    let input = Rc::new(RefCell::new(None));
    let mounted_input = Rc::clone(&input);
    let runs = Rc::new(Cell::new(0));
    let child_runs = Rc::clone(&runs);
    let instance = ComponentInstance::new(Dispatcher::immediate(), move |context| {
        context.mount_with(|child| {
            let input = child.receive(move |(), _context| {
                child_runs.set(child_runs.get() + 1);
            });
            assert!(EventSink::from(&input).send(()).is_ok());
            mounted_input.replace(Some(input));
        });

        let input = mounted_input.borrow();
        assert!(
            input
                .as_ref()
                .expect("child construction creates its event input")
                .send(())
                .is_ok()
        );
    });
    let input = input
        .borrow_mut()
        .take()
        .expect("child construction creates its event input");

    assert_eq!(runs.get(), 0);
    assert_eq!(
        instance.send(&input, ()),
        Ok(SubmissionOutcome::Completed(DispatchOutcome::Delivered))
    );
    assert_eq!(runs.get(), 1);
}

#[test]
fn an_instance_can_use_a_caller_owned_dispatcher_and_flush_boundary() {
    let mut dispatcher = FifoDispatcher::new();
    let input = Rc::new(RefCell::new(None));
    let mounted_input = Rc::clone(&input);
    let observed = Rc::new(RefCell::new(Vec::new()));
    let effect_output = Rc::clone(&observed);
    let mut instance = ComponentInstance::new(&dispatcher, move |context| {
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
    assert_eq!(instance.send(&input, 7), Ok(SubmissionOutcome::Deferred));
    assert!(dispatcher.deliver_next());
    assert_eq!(&*observed.borrow(), &[0]);

    instance.flush_effects().expect("valid updated Effect");
    assert_eq!(&*observed.borrow(), &[0, 7]);
    assert!(!instance.is_finished());
}

#[test]
fn inline_send_bypasses_the_default_dispatcher() {
    let mut dispatcher = FifoDispatcher::new();
    let input = Rc::new(RefCell::new(None));
    let mounted_input = Rc::clone(&input);
    let observed = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&observed);
    let instance = ComponentInstance::new(&dispatcher, move |context| {
        mounted_input.replace(Some(context.receive(move |value, _context| {
            output.borrow_mut().push(value);
        })));
    });
    let input = input
        .borrow_mut()
        .take()
        .expect("root construction creates its event input");

    assert_eq!(
        instance.send(&input, "deferred"),
        Ok(SubmissionOutcome::Deferred)
    );
    assert!(observed.borrow().is_empty());

    assert_eq!(instance.send_inline(&input, "inline"), Ok(()));
    assert_eq!(&*observed.borrow(), &["inline"]);

    assert!(dispatcher.deliver_next());
    assert_eq!(&*observed.borrow(), &["inline", "deferred"]);
}

#[test]
fn an_inline_connection_handles_a_publication_on_the_publishing_stack() {
    let dispatcher = FifoDispatcher::new();
    let input = Rc::new(RefCell::new(None));
    let mounted_input = Rc::clone(&input);
    let observed = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&observed);
    let instance = ComponentInstance::new(&dispatcher, move |context| {
        let (source, source_input) = context.event_source::<u32>();
        let sink = context.receive(move |value: u32, _context| {
            output.borrow_mut().push(value);
        });
        assert!(context.connect_cloned_inline(&source, sink));
        mounted_input.replace(Some(source_input));
    });
    let input = input
        .borrow_mut()
        .take()
        .expect("root construction creates its Source input");

    assert_eq!(instance.send_inline(input, 7_u32), Ok(()));
    assert_eq!(&*observed.borrow(), &[7]);
    assert!(!dispatcher.has_pending());
}

#[test]
fn an_instance_can_send_through_default_and_explicit_dispatchers() {
    let mut default_dispatcher = FifoDispatcher::new();
    let mut priority_dispatcher = FifoDispatcher::new();
    let priority_input: Dispatcher = priority_dispatcher.as_ref().clone();
    let input = Rc::new(RefCell::new(None));
    let mounted_input = Rc::clone(&input);
    let observed = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&observed);
    let instance = ComponentInstance::new(&default_dispatcher, move |context| {
        mounted_input.replace(Some(context.receive(move |value, _context| {
            output.borrow_mut().push(value);
        })));
    });
    let input = input
        .borrow_mut()
        .take()
        .expect("root construction creates its event input");

    assert_eq!(
        instance.send(&input, "default"),
        Ok(SubmissionOutcome::Deferred)
    );
    assert_eq!(
        instance.send_via(&priority_input, &input, "priority"),
        Ok(SubmissionOutcome::Deferred)
    );

    assert!(priority_dispatcher.deliver_next());
    assert_eq!(&*observed.borrow(), &["priority"]);
    assert!(default_dispatcher.deliver_next());
    assert_eq!(&*observed.borrow(), &["priority", "default"]);
}

#[test]
fn one_source_can_connect_through_default_and_explicit_dispatchers() {
    let mut default_dispatcher = FifoDispatcher::new();
    let mut priority_dispatcher = FifoDispatcher::new();
    let priority_input: Dispatcher = priority_dispatcher.as_ref().clone();
    let mounted_priority_input = priority_input.clone();
    let input = Rc::new(RefCell::new(None));
    let mounted_input = Rc::clone(&input);
    let observed = Rc::new(RefCell::new(Vec::new()));
    let default_output = Rc::clone(&observed);
    let priority_output = Rc::clone(&observed);
    let instance = ComponentInstance::new(&default_dispatcher, move |context| {
        let (source, source_input) = context.event_source::<u32>();
        let default_sink = context.receive(move |value, _context| {
            default_output.borrow_mut().push(("default", value));
        });
        let priority_sink = context.receive(move |value, _context| {
            priority_output.borrow_mut().push(("priority", value));
        });

        assert!(context.connect_map(&source, &default_sink, |value| *value));
        assert!(context.connect_map_via(
            &mounted_priority_input,
            &source,
            &priority_sink,
            |value| *value,
        ));
        mounted_input.replace(Some(source_input));
    });
    let input = input
        .borrow_mut()
        .take()
        .expect("root construction creates its Source input");

    assert_eq!(instance.send(&input, 7), Ok(SubmissionOutcome::Deferred));
    assert!(default_dispatcher.deliver_next());

    assert!(priority_dispatcher.deliver_next());
    assert_eq!(&*observed.borrow(), &[("priority", 7)]);
    assert!(default_dispatcher.deliver_next());
    assert_eq!(&*observed.borrow(), &[("priority", 7), ("default", 7)]);
}

#[test]
fn unmount_deactivates_descendants_before_the_current_handler_returns() {
    let root_input = Rc::new(RefCell::new(None));
    let mounted_root_input = Rc::clone(&root_input);
    let child_runs = Rc::new(Cell::new(0));
    let child_output = Rc::clone(&child_runs);
    let instance = ComponentInstance::new(Dispatcher::immediate(), move |context| {
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
