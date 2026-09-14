use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    rc::Rc,
};

use super::drain;
use theoria_event::{
    Dispatch, DispatchHandle, DispatchOutcome, DispatchQueue, DispatchStatus, Dispatcher,
    DynDispatch, EventHandler, EventSink, EventSource, EventTarget, OwningSink, SubmissionOutcome,
    TargetDispatch, TrackedDispatch, channel,
};

#[derive(Clone)]
struct TestDispatcher {
    pending: Rc<RefCell<VecDeque<DynDispatch<'static>>>>,
}

impl<D> Dispatcher<D> for TestDispatcher
where
    D: Dispatch + 'static,
{
    fn submit(&self, dispatch: D) -> Result<SubmissionOutcome, D> {
        self.pending
            .borrow_mut()
            .push_back(DynDispatch::new(dispatch));
        Ok(SubmissionOutcome::Deferred)
    }
}

struct RejectingDispatcher;

#[derive(Debug)]
struct TestDispatch {
    output: Rc<Cell<bool>>,
}

impl Dispatch for TestDispatch {
    fn run(self) -> DispatchOutcome {
        self.output.set(true);
        DispatchOutcome::Completed
    }
}

fn submit_event<E, D>(
    dispatcher: &D,
    sink: impl Into<EventTarget<E>>,
    event: E,
) -> Result<SubmissionOutcome, TargetDispatch<E>>
where
    D: Dispatcher<TargetDispatch<E>>,
{
    dispatcher.submit(sink.into().send(event))
}

fn submit_tracked<E, D>(
    dispatcher: &D,
    sink: impl Into<EventTarget<E>>,
    event: E,
) -> Result<DispatchHandle, TrackedDispatch<TargetDispatch<E>>>
where
    D: Dispatcher<TrackedDispatch<TargetDispatch<E>>>,
{
    let dispatch = sink.into().send(event).tracked();
    let handle = dispatch.handle();
    dispatcher.submit(dispatch).map(|_| handle)
}

impl<D: Dispatch> Dispatcher<D> for RejectingDispatcher {
    fn submit(&self, dispatch: D) -> Result<SubmissionOutcome, D> {
        Err(dispatch)
    }
}

#[test]
fn a_custom_dispatcher_can_defer_an_opaque_dispatch() {
    let dispatcher = TestDispatcher {
        pending: Rc::new(RefCell::new(VecDeque::new())),
    };
    let event_dispatcher = dispatcher.clone();
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let handler = EventHandler::new(move |value: u32| output.borrow_mut().push(value));
    let sink = EventTarget::from(&handler);

    assert_eq!(
        submit_event(&event_dispatcher, &sink, 7_u32).expect("Dispatcher accepts work"),
        SubmissionOutcome::Deferred
    );
    assert!(values.borrow().is_empty());
    let outcome = dispatcher
        .pending
        .borrow_mut()
        .pop_front()
        .expect("the Dispatch was deferred")
        .run();
    assert_eq!(outcome, DispatchOutcome::Completed);
    assert_eq!(&*values.borrow(), &[7]);
}

#[test]
fn rejection_returns_the_original_dispatch() {
    let event_dispatcher = RejectingDispatcher;
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let handler = EventHandler::new(move |value: u32| output.borrow_mut().push(value));
    let sink = EventTarget::from(&handler);

    let dispatch =
        submit_event(&event_dispatcher, &sink, 7_u32).expect_err("the Dispatcher rejects work");
    assert!(values.borrow().is_empty());
    assert_eq!(dispatch.run(), DispatchOutcome::Completed);
    assert_eq!(&*values.borrow(), &[7]);
}

#[test]
fn rejection_returns_an_executable_dyn_dispatch() {
    let output = Rc::new(Cell::new(false));
    let dispatch = RejectingDispatcher
        .submit(DynDispatch::new(TestDispatch {
            output: Rc::clone(&output),
        }))
        .expect_err("the Dispatcher rejects work");

    assert!(!output.get());
    assert_eq!(dispatch.run(), DispatchOutcome::Completed);
    assert!(output.get());
}

#[test]
fn tracked_rejection_returns_the_original_dispatch() {
    let event_dispatcher = RejectingDispatcher;
    let handler = EventHandler::new(|_: u32| {});
    let sink = EventTarget::from(&handler);

    let dispatch =
        submit_tracked(&event_dispatcher, &sink, 7_u32).expect_err("the Dispatcher rejects work");
    let handle = dispatch.handle();
    assert_eq!(handle.status(), DispatchStatus::Unstarted);
    assert_eq!(dispatch.run(), DispatchOutcome::Completed);
    assert_eq!(
        handle.status(),
        DispatchStatus::Finished(DispatchOutcome::Completed)
    );
}

#[test]
fn direct_delivery_runs_on_the_sending_stack() {
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let handler = EventHandler::new(move |value: u32| output.borrow_mut().push(value));

    assert_eq!(
        EventTarget::from(&handler).send(7_u32).run(),
        DispatchOutcome::Completed
    );
    assert_eq!(&*values.borrow(), &[7]);
}

#[test]
fn a_tracked_dispatch_can_be_cancelled_before_execution() {
    let dispatcher = TestDispatcher {
        pending: Rc::new(RefCell::new(VecDeque::new())),
    };
    let event_dispatcher = dispatcher.clone();
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let handler = EventHandler::new(move |value: u32| output.borrow_mut().push(value));
    let sink = EventTarget::from(&handler);

    let handle =
        submit_tracked(&event_dispatcher, &sink, 7_u32).expect("the Dispatcher accepts work");
    assert_eq!(handle.status(), DispatchStatus::Unstarted);
    assert!(handle.cancel());
    assert_eq!(
        handle.status(),
        DispatchStatus::Finished(DispatchOutcome::Cancelled)
    );
    assert!(!handle.cancel());
    let outcome = dispatcher
        .pending
        .borrow_mut()
        .pop_front()
        .expect("the Dispatch was deferred")
        .run();
    assert_eq!(outcome, DispatchOutcome::Cancelled);
    assert_eq!(
        handle.status(),
        DispatchStatus::Finished(DispatchOutcome::Cancelled)
    );
    assert!(values.borrow().is_empty());
}

#[test]
fn execution_reports_when_its_sink_was_dropped() {
    let dispatcher = TestDispatcher {
        pending: Rc::new(RefCell::new(VecDeque::new())),
    };
    let event_dispatcher = dispatcher.clone();
    let handler = EventHandler::new(|_: u32| {});
    let sink = EventTarget::from(&handler);

    assert!(submit_event(&event_dispatcher, &sink, 7_u32).is_ok());
    drop(handler);
    let outcome = dispatcher
        .pending
        .borrow_mut()
        .pop_front()
        .expect("the Dispatch was deferred")
        .run();

    assert_eq!(outcome, DispatchOutcome::SinkClosed);
}

#[test]
fn dropping_a_handle_does_not_cancel_its_dispatch() {
    let mut queue = DispatchQueue::new();
    let event_dispatcher = queue.dispatcher();
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let handler = EventHandler::new(move |value: u32| output.borrow_mut().push(value));
    let sink = EventTarget::from(&handler);

    let handle =
        submit_tracked(&event_dispatcher, &sink, 7_u32).expect("the Dispatcher accepts work");
    drop(handle);
    assert!(queue.run_next().is_some());
    assert_eq!(&*values.borrow(), &[7]);
}

#[test]
fn running_a_tracked_dispatch_finishes_its_handle() {
    let handler = EventHandler::new(|_: u32| {});
    let dispatch = EventTarget::from(&handler).send(7_u32).tracked();
    let handle = dispatch.handle();

    assert_eq!(dispatch.run(), DispatchOutcome::Completed);

    assert_eq!(
        handle.status(),
        DispatchStatus::Finished(DispatchOutcome::Completed)
    );
    assert!(!handle.cancel());
}

#[test]
fn a_tracked_deferred_submission_records_its_execution_outcome() {
    let mut queue = DispatchQueue::new();
    let event_dispatcher = queue.dispatcher();
    let handler = EventHandler::new(|_: u32| {});
    let sink = EventTarget::from(&handler);
    let handle =
        submit_tracked(&event_dispatcher, &sink, 7_u32).expect("the Dispatcher accepts work");

    assert_eq!(handle.status(), DispatchStatus::Unstarted);
    assert!(queue.run_next().is_some());
    assert_eq!(
        handle.status(),
        DispatchStatus::Finished(DispatchOutcome::Completed)
    );
}

#[test]
fn a_handler_observes_its_tracked_dispatch_as_executing() {
    let mut queue = DispatchQueue::new();
    let event_dispatcher = queue.dispatcher();
    let handle_slot = Rc::new(RefCell::new(None::<DispatchHandle>));
    let handle_from_handler = Rc::clone(&handle_slot);
    let observed = Rc::new(RefCell::new(Vec::new()));
    let observed_from_handler = Rc::clone(&observed);
    let cancellations = Rc::new(RefCell::new(Vec::new()));
    let cancellations_from_handler = Rc::clone(&cancellations);
    let handler = EventHandler::new(move |(): ()| {
        let handle = handle_from_handler.borrow();
        let handle = handle
            .as_ref()
            .expect("the Handle is installed before deferred execution");
        let status = handle.status();
        observed_from_handler.borrow_mut().push(status);
        cancellations_from_handler
            .borrow_mut()
            .push(handle.cancel());
    });
    let sink = EventTarget::from(&handler);
    let handle =
        submit_tracked(&event_dispatcher, &sink, ()).expect("the Dispatcher accepts the Dispatch");
    handle_slot.replace(Some(handle.clone()));

    assert!(queue.run_next().is_some());

    assert_eq!(&*observed.borrow(), &[DispatchStatus::Executing]);
    assert_eq!(&*cancellations.borrow(), &[false]);
    assert_eq!(
        handle.status(),
        DispatchStatus::Finished(DispatchOutcome::Completed)
    );
}

#[test]
fn a_finish_callback_panic_prevents_later_callbacks() {
    let mut queue = DispatchQueue::new();
    let event_dispatcher = queue.dispatcher();
    let handler = EventHandler::new(|_: u32| {});
    let sink = EventTarget::from(&handler);
    let handle = submit_tracked(&event_dispatcher, &sink, 7_u32)
        .expect("the Dispatcher accepts the Dispatch");
    let later_callback_ran = Rc::new(Cell::new(false));
    handle.on_finish(|_| panic!("finish panic"));
    let observed = Rc::clone(&later_callback_ran);
    handle.on_finish(move |_| observed.set(true));

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = queue.run_next();
    }));

    assert!(result.is_err());
    assert!(!later_callback_ran.get());
    assert_eq!(
        handle.status(),
        DispatchStatus::Finished(DispatchOutcome::Completed)
    );
}

#[test]
fn dropping_a_dispatcher_leaves_discarded_tracked_dispatches_unstarted() {
    let queue = DispatchQueue::new();
    let event_dispatcher = queue.dispatcher();
    let handler = EventHandler::new(|_: u32| {});
    let sink = EventTarget::from(&handler);
    let handle = submit_tracked(&event_dispatcher, &sink, 7_u32)
        .expect("the Dispatcher accepts the Dispatch");
    let outcomes = Rc::new(RefCell::new(Vec::new()));
    let observed = Rc::clone(&outcomes);
    handle.on_finish(move |outcome| observed.borrow_mut().push(outcome));

    drop(queue);

    assert_eq!(handle.status(), DispatchStatus::Unstarted);
    assert!(outcomes.borrow().is_empty());
}

#[test]
fn a_panicking_sink_leaves_its_tracked_dispatch_executing() {
    let mut queue = DispatchQueue::new();
    let event_dispatcher = queue.dispatcher();
    let handler = EventHandler::new(|(): ()| panic!("sink panic"));
    let sink = EventTarget::from(&handler);
    let handle =
        submit_tracked(&event_dispatcher, &sink, ()).expect("the Dispatcher accepts the Dispatch");
    let callbacks = Rc::new(RefCell::new(Vec::new()));
    let observed = Rc::clone(&callbacks);
    handle.on_finish(move |outcome| observed.borrow_mut().push(outcome));

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = queue.run_next();
    }));

    assert!(result.is_err());
    assert_eq!(handle.status(), DispatchStatus::Executing);
    assert!(callbacks.borrow().is_empty());
}

#[test]
fn a_handle_notifies_finish_after_deferred_execution() {
    let mut queue = DispatchQueue::new();
    let event_dispatcher = queue.dispatcher();
    let handler = EventHandler::new(|_: u32| {});
    let sink = EventTarget::from(&handler);
    let handle =
        submit_tracked(&event_dispatcher, &sink, 7_u32).expect("the Dispatcher accepts work");
    let outcomes = Rc::new(RefCell::new(Vec::new()));
    let observed = Rc::clone(&outcomes);

    handle.on_finish(move |outcome| observed.borrow_mut().push(outcome));
    assert!(outcomes.borrow().is_empty());

    assert!(queue.run_next().is_some());
    assert_eq!(&*outcomes.borrow(), &[DispatchOutcome::Completed]);
}

#[test]
fn finish_callbacks_survive_handle_drop_and_keep_registration_order() {
    let mut queue = DispatchQueue::new();
    let event_dispatcher = queue.dispatcher();
    let handler = EventHandler::new(|_: u32| {});
    let sink = EventTarget::from(&handler);
    let handle =
        submit_tracked(&event_dispatcher, &sink, 7_u32).expect("the Dispatcher accepts work");
    let notifications = Rc::new(RefCell::new(Vec::new()));
    let first = Rc::clone(&notifications);
    handle.on_finish(move |outcome| first.borrow_mut().push((1, outcome)));
    let second = Rc::clone(&notifications);
    handle.on_finish(move |outcome| second.borrow_mut().push((2, outcome)));
    drop(handle);

    assert!(queue.run_next().is_some());
    assert_eq!(
        &*notifications.borrow(),
        &[
            (1, DispatchOutcome::Completed),
            (2, DispatchOutcome::Completed)
        ]
    );
}

#[test]
fn a_handle_notifies_immediately_when_its_dispatch_already_finished() {
    let handler = EventHandler::new(|_: u32| {});
    let sink = EventTarget::from(&handler);
    let dispatch = sink.send(7_u32).tracked();
    let handle = dispatch.handle();
    assert_eq!(dispatch.run(), DispatchOutcome::Completed);
    let outcome = Rc::new(RefCell::new(None));
    let observed = Rc::clone(&outcome);

    handle.on_finish(move |outcome| {
        observed.replace(Some(outcome));
    });

    assert_eq!(*outcome.borrow(), Some(DispatchOutcome::Completed));
}

#[test]
fn cancelling_a_dispatch_notifies_its_finish() {
    let dispatcher = TestDispatcher {
        pending: Rc::new(RefCell::new(VecDeque::new())),
    };
    let event_dispatcher = dispatcher;
    let handler = EventHandler::new(|_: u32| {});
    let sink = EventTarget::from(&handler);
    let handle =
        submit_tracked(&event_dispatcher, &sink, 7_u32).expect("the Dispatcher accepts work");
    let outcome = Rc::new(RefCell::new(None));
    let observed = Rc::clone(&outcome);
    handle.on_finish(move |outcome| {
        observed.replace(Some(outcome));
    });

    assert!(handle.cancel());
    assert_eq!(*outcome.borrow(), Some(DispatchOutcome::Cancelled));
}

#[test]
fn one_sink_can_run_inline_or_be_submitted_to_a_dispatcher() {
    let mut queue = DispatchQueue::new();
    let queued = queue.dispatcher();
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let handler = EventHandler::new(move |value: u32| output.borrow_mut().push(value));
    let sink = EventTarget::from(&handler);

    assert_eq!(
        submit_event(&queued, &sink, 1_u32).expect("Dispatcher accepts work"),
        SubmissionOutcome::Deferred
    );
    assert_eq!(sink.send(2_u32).run(), DispatchOutcome::Completed);
    assert_eq!(&*values.borrow(), &[2]);
    assert!(queue.run_next().is_some());
    assert_eq!(&*values.borrow(), &[2, 1]);
}

#[test]
fn a_handler_can_build_and_use_a_source_while_handling_one_dispatch() {
    let mut queue = DispatchQueue::new();
    let event_dispatcher = queue.dispatcher();
    let values = Rc::new(RefCell::new(Vec::new()));
    let dynamic = Rc::new(RefCell::new(None));
    let dynamic_from_handler = Rc::clone(&dynamic);
    let output = Rc::clone(&values);
    let nested_dispatcher = event_dispatcher.clone();
    let trigger_handler = EventHandler::new(move |(): ()| {
        let handler_output = Rc::clone(&output);
        let handler = EventHandler::new(move |value: u32| {
            handler_output.borrow_mut().push(value);
        });
        let sink = EventTarget::from(&handler);
        let (provider, source) = channel::<u32>();
        let input = EventTarget::from(&provider);
        let subscription = source
            .clone()
            .connect(OwningSink::new(sink))
            .via(nested_dispatcher.clone())
            .expect("live provider");
        dynamic_from_handler.replace(Some((provider, source, handler, subscription)));
        assert!(submit_event(&nested_dispatcher, &input, 9_u32).is_ok());
    });
    let trigger = EventTarget::from(&trigger_handler);

    assert!(submit_event(&event_dispatcher, &trigger, ()).is_ok());
    drain(&mut queue);
    assert!(dynamic.borrow().is_some());
    assert_eq!(&*values.borrow(), &[9]);
}
