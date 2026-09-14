use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use super::drain;
use theoria_event::{
    DispatchOutcome, Dispatcher, EventDispatch, EventDispatcher, EventHandler, EventSource,
    EventTarget, FifoDispatcher, OwningSink, SubmissionOutcome, channel,
};

struct TestDispatcher {
    pending: RefCell<Vec<EventDispatch<'static>>>,
}

fn submit_event<E: 'static>(
    dispatcher: &Dispatcher,
    sink: impl Into<EventTarget<E>>,
    event: E,
) -> Result<SubmissionOutcome, EventDispatch<'static>> {
    dispatcher.submit(sink.into().send(event))
}

impl EventDispatcher<EventDispatch<'static>> for TestDispatcher {
    fn submit(
        &self,
        dispatch: EventDispatch<'static>,
    ) -> Result<SubmissionOutcome, EventDispatch<'static>> {
        self.pending.borrow_mut().push(dispatch);
        Ok(SubmissionOutcome::Deferred)
    }
}

#[test]
fn a_sink_alive_at_dispatch_start_finishes_that_dispatch() {
    let mut dispatcher = FifoDispatcher::new();
    let event_dispatcher: Dispatcher = dispatcher.as_ref().clone();
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let handler = EventHandler::new(move |value: u32| output.borrow_mut().push(value));
    let sink = EventTarget::from(&handler);
    let handler = Rc::new(RefCell::new(Some(handler)));
    let callback_handler = Rc::clone(&handler);
    let (provider, source) = channel::<u32>();
    source
        .connect(OwningSink::new(sink.contramap(move |event| {
            callback_handler.borrow_mut().take();
            event
        })))
        .via(event_dispatcher.clone())
        .expect("live provider")
        .detach();

    assert!(submit_event(&event_dispatcher, &provider, 1_u32).is_ok());
    assert!(dispatcher.deliver_next());
    assert!(dispatcher.deliver_next());
    assert!(!dispatcher.deliver_next());
    assert_eq!(&*values.borrow(), &[1]);
    assert!(submit_event(&event_dispatcher, &sink, 2_u32).is_ok());
    drain(&mut dispatcher);
    assert_eq!(&*values.borrow(), &[1]);
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

    let mut dispatcher = FifoDispatcher::new();
    let event_dispatcher: Dispatcher = dispatcher.as_ref().clone();
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let handler = EventHandler::new(move |value: Output| {
        output.borrow_mut().push(value);
    });
    let sink = EventTarget::from(&handler);
    let accepting = sink.accepting::<Input>();

    assert!(submit_event(&event_dispatcher, &accepting, Input(7)).is_ok());
    assert!(values.borrow().is_empty());
    drain(&mut dispatcher);

    assert_eq!(&*values.borrow(), &[Output(7)]);
}

#[test]
fn a_contramap_sink_converts_its_input_during_dispatch_execution() {
    let mut dispatcher = FifoDispatcher::new();
    let event_dispatcher: Dispatcher = dispatcher.as_ref().clone();
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let handler = EventHandler::new(move |value: usize| output.borrow_mut().push(value));
    let sink = EventTarget::from(&handler).contramap(|value: String| value.len());

    assert!(submit_event(&event_dispatcher, &sink, String::from("letter")).is_ok());
    assert!(values.borrow().is_empty());
    drain(&mut dispatcher);

    assert_eq!(&*values.borrow(), &[6]);
}

#[test]
fn a_filtered_sink_discards_selected_inputs_during_dispatch_execution() {
    let dispatcher = Rc::new(TestDispatcher {
        pending: RefCell::new(Vec::new()),
    });
    let event_dispatcher = Dispatcher::new(dispatcher.clone());
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

    assert_eq!(first, DispatchOutcome::Delivered);
    assert_eq!(second, DispatchOutcome::Delivered);
    assert_eq!(mappings.get(), 2);
    assert_eq!(&*values.borrow(), &[2]);
}

#[test]
fn an_expired_filtered_sink_is_unavailable_without_running_its_predicate() {
    let predicate_calls = Rc::new(Cell::new(0));
    let observed_calls = Rc::clone(&predicate_calls);
    let handler = EventHandler::new(|_: u32| {});
    let sink = EventTarget::from(&handler).filter(move |_| {
        observed_calls.set(observed_calls.get() + 1);
        false
    });
    drop(handler);

    assert_eq!(sink.send(7_u32).run(), DispatchOutcome::SinkUnavailable);
    assert_eq!(predicate_calls.get(), 0);
}

#[test]
fn a_contramap_filter_sink_converts_only_selected_input_values() {
    let mut dispatcher = FifoDispatcher::new();
    let event_dispatcher: Dispatcher = dispatcher.as_ref().clone();
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let handler = EventHandler::new(move |value: u32| output.borrow_mut().push(value));
    let sink =
        EventTarget::from(&handler).contramap_filter(|value: String| value.parse::<u32>().ok());

    assert!(submit_event(&event_dispatcher, &sink, String::from("not a number")).is_ok());
    assert!(submit_event(&event_dispatcher, &sink, String::from("42")).is_ok());
    drain(&mut dispatcher);

    assert_eq!(&*values.borrow(), &[42]);
}
