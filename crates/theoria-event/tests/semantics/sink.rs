use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use super::drain;
use theoria_event::{
    Dispatch, DispatchOutcome, DispatchQueue, Dispatcher, DynDispatch, EventHandler, EventSink,
    EventSource, EventTarget, OwningSink, SubmissionOutcome, channel,
};

#[derive(Clone)]
struct TestDispatcher {
    pending: Rc<RefCell<Vec<DynDispatch<'static>>>>,
}

fn submit_event<E, S, D>(
    dispatcher: &D,
    sink: S,
    event: E,
) -> Result<SubmissionOutcome, S::Dispatch>
where
    S: EventSink<E>,
    D: Dispatcher<S::Dispatch>,
{
    dispatcher.submit(sink.send(event))
}

impl<D> Dispatcher<D> for TestDispatcher
where
    D: Dispatch + 'static,
{
    fn submit(&self, dispatch: D) -> Result<SubmissionOutcome, D> {
        self.pending.borrow_mut().push(DynDispatch::new(dispatch));
        Ok(SubmissionOutcome::Deferred)
    }
}

#[test]
fn dropping_a_target_during_generic_contramap_prevents_delivery() {
    let mut queue = DispatchQueue::new();
    let event_dispatcher = queue.dispatcher();
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let handler = EventHandler::new(move |value: u32| output.borrow_mut().push(value));
    let sink = EventTarget::from(&handler);
    let handler = Rc::new(RefCell::new(Some(handler)));
    let callback_handler = Rc::clone(&handler);
    let (provider, source) = channel::<u32>();
    source
        .connect(OwningSink::new(sink.clone().contramap(move |event| {
            callback_handler.borrow_mut().take();
            event
        })))
        .via(event_dispatcher.clone())
        .expect("live provider")
        .detach();

    assert!(submit_event(&event_dispatcher, EventTarget::from(&provider), 1_u32).is_ok());
    assert!(queue.run_next().is_some());
    assert!(queue.run_next().is_some());
    assert!(queue.run_next().is_none());
    assert!(values.borrow().is_empty());
    assert!(submit_event(&event_dispatcher, &sink, 2_u32).is_ok());
    drain(&mut queue);
    assert!(values.borrow().is_empty());
}

#[test]
fn an_accepting_sink_converts_its_input_during_dispatch_execution() {
    struct Input(u32);

    #[derive(Debug, PartialEq)]
    struct Output(u32);

    impl From<Input> for Output {
        fn from(value: Input) -> Self {
            Self(value.0)
        }
    }

    let mut queue = DispatchQueue::new();
    let event_dispatcher = queue.dispatcher();
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let handler = EventHandler::new(move |value: Output| {
        output.borrow_mut().push(value);
    });
    let sink = EventTarget::from(&handler);
    let accepting = sink.accepting::<Input>();

    assert!(submit_event(&event_dispatcher, &accepting, Input(7)).is_ok());
    assert!(values.borrow().is_empty());
    drain(&mut queue);

    assert_eq!(&*values.borrow(), &[Output(7)]);
}

#[test]
fn a_contramap_sink_converts_its_input_during_dispatch_execution() {
    let mut queue = DispatchQueue::new();
    let event_dispatcher = queue.dispatcher();
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let handler = EventHandler::new(move |value: usize| output.borrow_mut().push(value));
    let sink = EventTarget::from(&handler).contramap(|value: String| value.len());

    assert!(submit_event(&event_dispatcher, &sink, String::from("letter")).is_ok());
    assert!(values.borrow().is_empty());
    drain(&mut queue);

    assert_eq!(&*values.borrow(), &[6]);
}

#[test]
fn dyn_dispatch_erases_different_concrete_dispatch_types_at_a_storage_boundary() {
    let values = Rc::new(RefCell::new(Vec::new()));
    let first_output = Rc::clone(&values);
    let first = EventHandler::new(move |value: u32| first_output.borrow_mut().push(value));
    let second_output = Rc::clone(&values);
    let second =
        EventHandler::new(move |value: usize| second_output.borrow_mut().push(value as u32));

    let dispatches = vec![
        DynDispatch::new(EventTarget::from(&first).send(3_u32)),
        DynDispatch::new(
            EventTarget::from(&second)
                .contramap(|value: String| value.len())
                .send(String::from("seven")),
        ),
    ];
    for dispatch in dispatches {
        assert_eq!(dispatch.run(), DispatchOutcome::Completed);
    }

    assert_eq!(&*values.borrow(), &[3, 5]);
}

#[test]
fn an_adapted_sink_is_cloneable_when_its_sink_and_mapping_are_cloneable() {
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let handler = EventHandler::new(move |value: usize| output.borrow_mut().push(value));
    let sink = EventTarget::from(&handler).contramap(|value: String| value.len());

    assert_eq!(
        (&sink).send(String::from("first")).run(),
        DispatchOutcome::Completed
    );
    assert_eq!(
        sink.send(String::from("second")).run(),
        DispatchOutcome::Completed
    );
    assert_eq!(&*values.borrow(), &[5, 6]);
}

#[test]
fn a_filtered_sink_discards_selected_inputs_during_dispatch_execution() {
    let dispatcher = TestDispatcher {
        pending: Rc::new(RefCell::new(Vec::new())),
    };
    let event_dispatcher = dispatcher.clone();
    let mappings = Rc::new(Cell::new(0));
    let mapped = Rc::clone(&mappings);
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let handler = EventHandler::new(move |value: u32| output.borrow_mut().push(value));
    let sink = EventTarget::from(&handler).filter(move |value| {
        mapped.set(mapped.get() + 1);
        value % 2 == 0
    });

    assert!(submit_event(&event_dispatcher, &sink, 1_u32).is_ok());
    assert!(submit_event(&event_dispatcher, &sink, 2_u32).is_ok());
    assert_eq!(
        mappings.get(),
        0,
        "filtering must happen during Dispatch execution"
    );
    let first = dispatcher.pending.borrow_mut().remove(0).run();
    let second = dispatcher.pending.borrow_mut().remove(0).run();

    assert_eq!(first, DispatchOutcome::Completed);
    assert_eq!(second, DispatchOutcome::Completed);
    assert_eq!(mappings.get(), 2);
    assert_eq!(&*values.borrow(), &[2]);
}

#[test]
fn a_filtered_out_event_does_not_observe_underlying_target_liveness() {
    let predicate_calls = Rc::new(Cell::new(0));
    let observed_calls = Rc::clone(&predicate_calls);
    let handler = EventHandler::new(|_: u32| {});
    let sink = EventTarget::from(&handler).filter(move |_| {
        observed_calls.set(observed_calls.get() + 1);
        false
    });
    drop(handler);

    assert_eq!(sink.send(7_u32).run(), DispatchOutcome::Completed);
    assert_eq!(predicate_calls.get(), 1);
}

#[test]
fn a_contramap_filter_sink_converts_only_selected_input_values() {
    let mut queue = DispatchQueue::new();
    let event_dispatcher = queue.dispatcher();
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let handler = EventHandler::new(move |value: u32| output.borrow_mut().push(value));
    let sink =
        EventTarget::from(&handler).contramap_filter(|value: String| value.parse::<u32>().ok());

    assert!(submit_event(&event_dispatcher, &sink, String::from("not a number")).is_ok());
    assert!(submit_event(&event_dispatcher, &sink, String::from("42")).is_ok());
    drain(&mut queue);

    assert_eq!(&*values.borrow(), &[42]);
}
