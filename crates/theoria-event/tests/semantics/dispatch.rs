use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    rc::Rc,
};

use super::drain;
use theoria_event::{
    Dispatch, DispatchHandle, DispatchOutcome, DispatchStatus, DynDispatch, EventDispatcher,
    EventHandler, EventSource, EventTarget, FifoDispatcher, OwningSink, SubmissionOutcome,
    TargetDispatch, TrackedDispatch, channel,
};

#[derive(Clone)]
struct TestDispatcher {
    pending: Rc<RefCell<VecDeque<DynDispatch<'static>>>>,
}

impl<D> EventDispatcher<D> for TestDispatcher
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
        DispatchOutcome::Delivered
    }
}

fn submit_event<E, D>(
    dispatcher: &D,
    sink: impl Into<EventTarget<E>>,
    event: E,
) -> Result<SubmissionOutcome, TargetDispatch<E>>
where
    D: EventDispatcher<TargetDispatch<E>>,
{
    dispatcher.submit(sink.into().send(event))
}

fn submit_tracked<E, D>(
    dispatcher: &D,
    sink: impl Into<EventTarget<E>>,
    event: E,
) -> Result<DispatchHandle, TrackedDispatch<TargetDispatch<E>>>
where
    D: EventDispatcher<TrackedDispatch<TargetDispatch<E>>>,
{
    let dispatch = sink.into().send(event).tracked();
    let handle = dispatch.handle();
    dispatcher.submit(dispatch).map(|_| handle)
}

impl<D: Dispatch> EventDispatcher<D> for RejectingDispatcher {
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
    assert_eq!(outcome, DispatchOutcome::Delivered);
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
    assert_eq!(dispatch.run(), DispatchOutcome::Delivered);
    assert_eq!(&*values.borrow(), &[7]);
}

#[test]
fn a_recoverable_dyn_dispatch_restores_its_concrete_dispatch() {
    let output = Rc::new(Cell::new(false));

    let dispatch = RejectingDispatcher
        .submit(DynDispatch::recoverable(TestDispatch {
            output: Rc::clone(&output),
        }))
        .expect_err("the Dispatcher rejects work")
        .downcast::<TestDispatch>()
        .expect("the recoverable erasure retains its concrete Dispatch");

    assert!(!output.get());
    assert_eq!(dispatch.run(), DispatchOutcome::Delivered);
    assert!(output.get());
}

#[test]
fn ordinary_dyn_dispatch_erasure_is_not_downcastable() {
    let output = Rc::new(Cell::new(false));
    let dispatch = DynDispatch::new(TestDispatch {
        output: Rc::clone(&output),
    });

    let dispatch = dispatch
        .downcast::<TestDispatch>()
        .expect_err("ordinary erasure does not retain a recoverable concrete value");
    assert_eq!(dispatch.run(), DispatchOutcome::Delivered);
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
    assert_eq!(handle.status(), DispatchStatus::Pending);
    assert_eq!(dispatch.run(), DispatchOutcome::Delivered);
    assert_eq!(
        handle.status(),
        DispatchStatus::Finished(DispatchOutcome::Delivered)
    );
}

#[test]
fn direct_delivery_runs_on_the_sending_stack() {
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let handler = EventHandler::new(move |value: u32| output.borrow_mut().push(value));

    assert_eq!(
        EventTarget::from(&handler).send(7_u32).run(),
        DispatchOutcome::Delivered
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
    assert_eq!(handle.status(), DispatchStatus::Pending);
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

    assert_eq!(outcome, DispatchOutcome::SinkUnavailable);
}

#[test]
fn dropping_a_handle_does_not_cancel_its_dispatch() {
    let mut dispatcher = FifoDispatcher::new();
    let event_dispatcher = dispatcher.sender();
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let handler = EventHandler::new(move |value: u32| output.borrow_mut().push(value));
    let sink = EventTarget::from(&handler);

    let handle =
        submit_tracked(&event_dispatcher, &sink, 7_u32).expect("the Dispatcher accepts work");
    drop(handle);
    assert!(dispatcher.deliver_next());
    assert_eq!(&*values.borrow(), &[7]);
}

#[test]
fn running_a_tracked_dispatch_finishes_its_handle() {
    let handler = EventHandler::new(|_: u32| {});
    let dispatch = EventTarget::from(&handler).send(7_u32).tracked();
    let handle = dispatch.handle();

    assert_eq!(dispatch.run(), DispatchOutcome::Delivered);

    assert_eq!(
        handle.status(),
        DispatchStatus::Finished(DispatchOutcome::Delivered)
    );
    assert!(!handle.cancel());
}

#[test]
fn a_tracked_deferred_submission_records_its_execution_outcome() {
    let mut dispatcher = FifoDispatcher::new();
    let event_dispatcher = dispatcher.sender();
    let handler = EventHandler::new(|_: u32| {});
    let sink = EventTarget::from(&handler);
    let handle =
        submit_tracked(&event_dispatcher, &sink, 7_u32).expect("the Dispatcher accepts work");

    assert_eq!(handle.status(), DispatchStatus::Pending);
    assert!(dispatcher.deliver_next());
    assert_eq!(
        handle.status(),
        DispatchStatus::Finished(DispatchOutcome::Delivered)
    );
}

#[test]
fn a_handler_observes_its_tracked_dispatch_as_executing() {
    let mut dispatcher = FifoDispatcher::new();
    let event_dispatcher = dispatcher.sender();
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

    assert!(dispatcher.deliver_next());

    assert_eq!(&*observed.borrow(), &[DispatchStatus::Executing]);
    assert_eq!(&*cancellations.borrow(), &[false]);
    assert_eq!(
        handle.status(),
        DispatchStatus::Finished(DispatchOutcome::Delivered)
    );
}

#[test]
fn a_completion_callback_panic_prevents_later_callbacks() {
    let mut dispatcher = FifoDispatcher::new();
    let event_dispatcher = dispatcher.sender();
    let handler = EventHandler::new(|_: u32| {});
    let sink = EventTarget::from(&handler);
    let handle = submit_tracked(&event_dispatcher, &sink, 7_u32)
        .expect("the Dispatcher accepts the Dispatch");
    let later_callback_ran = Rc::new(Cell::new(false));
    handle.on_complete(|_| panic!("completion panic"));
    let observed = Rc::clone(&later_callback_ran);
    handle.on_complete(move |_| observed.set(true));

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = dispatcher.deliver_next();
    }));

    assert!(result.is_err());
    assert!(!later_callback_ran.get());
    assert_eq!(
        handle.status(),
        DispatchStatus::Finished(DispatchOutcome::Delivered)
    );
}

#[test]
fn dropping_a_dispatcher_cancels_its_pending_tracked_dispatches() {
    let dispatcher = FifoDispatcher::new();
    let event_dispatcher = dispatcher.sender();
    let handler = EventHandler::new(|_: u32| {});
    let sink = EventTarget::from(&handler);
    let handle = submit_tracked(&event_dispatcher, &sink, 7_u32)
        .expect("the Dispatcher accepts the Dispatch");
    let outcomes = Rc::new(RefCell::new(Vec::new()));
    let observed = Rc::clone(&outcomes);
    handle.on_complete(move |outcome| observed.borrow_mut().push(outcome));

    drop(dispatcher);

    assert_eq!(
        handle.status(),
        DispatchStatus::Finished(DispatchOutcome::Cancelled)
    );
    assert_eq!(&*outcomes.borrow(), &[DispatchOutcome::Cancelled]);
}

#[test]
fn a_panicking_sink_leaves_its_tracked_dispatch_executing() {
    let mut dispatcher = FifoDispatcher::new();
    let event_dispatcher = dispatcher.sender();
    let handler = EventHandler::new(|(): ()| panic!("sink panic"));
    let sink = EventTarget::from(&handler);
    let handle =
        submit_tracked(&event_dispatcher, &sink, ()).expect("the Dispatcher accepts the Dispatch");
    let callbacks = Rc::new(RefCell::new(Vec::new()));
    let observed = Rc::clone(&callbacks);
    handle.on_complete(move |outcome| observed.borrow_mut().push(outcome));

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = dispatcher.deliver_next();
    }));

    assert!(result.is_err());
    assert_eq!(handle.status(), DispatchStatus::Executing);
    assert!(callbacks.borrow().is_empty());
}

#[test]
fn a_handle_notifies_completion_after_deferred_execution() {
    let mut dispatcher = FifoDispatcher::new();
    let event_dispatcher = dispatcher.sender();
    let handler = EventHandler::new(|_: u32| {});
    let sink = EventTarget::from(&handler);
    let handle =
        submit_tracked(&event_dispatcher, &sink, 7_u32).expect("the Dispatcher accepts work");
    let outcomes = Rc::new(RefCell::new(Vec::new()));
    let observed = Rc::clone(&outcomes);

    handle.on_complete(move |outcome| observed.borrow_mut().push(outcome));
    assert!(outcomes.borrow().is_empty());

    assert!(dispatcher.deliver_next());
    assert_eq!(&*outcomes.borrow(), &[DispatchOutcome::Delivered]);
}

#[test]
fn completion_callbacks_survive_handle_drop_and_keep_registration_order() {
    let mut dispatcher = FifoDispatcher::new();
    let event_dispatcher = dispatcher.sender();
    let handler = EventHandler::new(|_: u32| {});
    let sink = EventTarget::from(&handler);
    let handle =
        submit_tracked(&event_dispatcher, &sink, 7_u32).expect("the Dispatcher accepts work");
    let notifications = Rc::new(RefCell::new(Vec::new()));
    let first = Rc::clone(&notifications);
    handle.on_complete(move |outcome| first.borrow_mut().push((1, outcome)));
    let second = Rc::clone(&notifications);
    handle.on_complete(move |outcome| second.borrow_mut().push((2, outcome)));
    drop(handle);

    assert!(dispatcher.deliver_next());
    assert_eq!(
        &*notifications.borrow(),
        &[
            (1, DispatchOutcome::Delivered),
            (2, DispatchOutcome::Delivered)
        ]
    );
}

#[test]
fn a_handle_notifies_immediately_when_its_dispatch_already_completed() {
    let handler = EventHandler::new(|_: u32| {});
    let sink = EventTarget::from(&handler);
    let dispatch = sink.send(7_u32).tracked();
    let handle = dispatch.handle();
    assert_eq!(dispatch.run(), DispatchOutcome::Delivered);
    let outcome = Rc::new(RefCell::new(None));
    let observed = Rc::clone(&outcome);

    handle.on_complete(move |completed| {
        observed.replace(Some(completed));
    });

    assert_eq!(*outcome.borrow(), Some(DispatchOutcome::Delivered));
}

#[test]
fn cancelling_a_dispatch_notifies_its_completion() {
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
    handle.on_complete(move |completed| {
        observed.replace(Some(completed));
    });

    assert!(handle.cancel());
    assert_eq!(*outcome.borrow(), Some(DispatchOutcome::Cancelled));
}

#[test]
fn one_sink_can_run_inline_or_be_submitted_to_a_dispatcher() {
    let mut dispatcher = FifoDispatcher::new();
    let queued = dispatcher.sender();
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let handler = EventHandler::new(move |value: u32| output.borrow_mut().push(value));
    let sink = EventTarget::from(&handler);

    assert_eq!(
        submit_event(&queued, &sink, 1_u32).expect("Dispatcher accepts work"),
        SubmissionOutcome::Deferred
    );
    assert_eq!(sink.send(2_u32).run(), DispatchOutcome::Delivered);
    assert_eq!(&*values.borrow(), &[2]);
    assert!(dispatcher.deliver_next());
    assert_eq!(&*values.borrow(), &[2, 1]);
}

#[test]
fn a_handler_can_build_and_use_a_source_while_handling_one_dispatch() {
    let mut dispatcher = FifoDispatcher::new();
    let event_dispatcher = dispatcher.sender();
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
    drain(&mut dispatcher);
    assert!(dynamic.borrow().is_some());
    assert_eq!(&*values.borrow(), &[9]);
}

#[test]
fn dropping_a_fifo_dispatcher_makes_its_capability_reject_submissions() {
    let dispatcher = FifoDispatcher::new();
    let event_dispatcher = dispatcher.sender();
    let (provider, _source) = channel::<u32>();
    let handler = EventHandler::new(|_: u32| {});
    let independent_sink = EventTarget::from(&handler);
    drop(dispatcher);

    assert!(submit_event(&event_dispatcher, &provider, 1_u32).is_err());
    assert_eq!(
        independent_sink.send(1_u32).run(),
        DispatchOutcome::Delivered
    );
}

#[test]
fn heterogeneous_event_types_share_one_fifo() {
    let mut dispatcher = FifoDispatcher::new();
    let event_dispatcher = dispatcher.sender();
    let order = Rc::new(RefCell::new(Vec::new()));
    let number_order = Rc::clone(&order);
    let text_order = Rc::clone(&order);
    let number_handler = EventHandler::new(move |value: u32| {
        number_order.borrow_mut().push(value.to_string());
    });
    let number_sink = EventTarget::from(&number_handler);
    let text_handler = EventHandler::new(move |value: String| {
        text_order.borrow_mut().push(value);
    });
    let text_sink = EventTarget::from(&text_handler);

    assert!(submit_event(&event_dispatcher, &number_sink, 1_u32).is_ok());
    assert!(submit_event(&event_dispatcher, &text_sink, String::from("two")).is_ok());
    drain(&mut dispatcher);

    assert_eq!(&*order.borrow(), &["1", "two"]);
}

#[test]
fn work_submitted_while_a_dispatch_executes_joins_the_fifo_tail() {
    let mut dispatcher = FifoDispatcher::new();
    let event_dispatcher = dispatcher.sender();
    let order = Rc::new(RefCell::new(Vec::new()));

    let last_order = Rc::clone(&order);
    let last_handler = EventHandler::new(move |(): ()| {
        last_order.borrow_mut().push("last");
    });
    let last_sink = EventTarget::from(&last_handler);
    let nested_dispatcher = event_dispatcher.clone();
    let first_order = Rc::clone(&order);
    let first_handler = EventHandler::new(move |(): ()| {
        first_order.borrow_mut().push("first");
        assert!(submit_event(&nested_dispatcher, &last_sink, ()).is_ok());
    });
    let first_sink = EventTarget::from(&first_handler);
    let second_order = Rc::clone(&order);
    let second_handler = EventHandler::new(move |(): ()| {
        second_order.borrow_mut().push("second");
    });
    let second_sink = EventTarget::from(&second_handler);

    assert!(submit_event(&event_dispatcher, &first_sink, ()).is_ok());
    assert!(submit_event(&event_dispatcher, &second_sink, ()).is_ok());
    drain(&mut dispatcher);

    assert_eq!(&*order.borrow(), &["first", "second", "last"]);
}
