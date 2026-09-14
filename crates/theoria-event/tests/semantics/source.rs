use std::{cell::Cell, rc::Rc};

use theoria_event::{
    Dispatch, DispatchOutcome, EventHandler, EventSink, EventSource, FifoDispatcher, Sink,
    Subscription, channel,
};

struct BorrowingSource<'value> {
    value: &'value str,
}

struct BorrowedLengthSink {
    output: Rc<Cell<usize>>,
}

struct BorrowedLengthDispatch<'event> {
    event: &'event str,
    output: Rc<Cell<usize>>,
}

impl Dispatch for BorrowedLengthDispatch<'_> {
    fn run(self) -> DispatchOutcome {
        self.output.set(self.event.len());
        DispatchOutcome::Delivered
    }
}

impl<'event> Sink<&'event str> for BorrowedLengthSink {
    type Dispatch = BorrowedLengthDispatch<'event>;

    fn send(&self, event: &'event str) -> Self::Dispatch {
        BorrowedLengthDispatch {
            event,
            output: Rc::clone(&self.output),
        }
    }
}

struct OwnedSink {
    output: Rc<Cell<u32>>,
}

struct OwnedDispatch {
    event: u32,
    output: Rc<Cell<u32>>,
}

impl Dispatch for OwnedDispatch {
    fn run(self) -> DispatchOutcome {
        self.output.set(self.event);
        DispatchOutcome::Delivered
    }
}

impl Sink<u32> for OwnedSink {
    type Dispatch = OwnedDispatch;

    fn send(&self, event: u32) -> Self::Dispatch {
        OwnedDispatch {
            event,
            output: Rc::clone(&self.output),
        }
    }
}

impl EventSource for BorrowingSource<'_> {
    type Event<'event> = &'event str;

    fn subscribe<F>(self, receive: F) -> Option<Subscription>
    where
        F: for<'event> Fn(Self::Event<'event>) + 'static,
    {
        receive(self.value);
        Some(Subscription::new(|| {}))
    }
}

#[test]
fn a_user_defined_source_can_lend_a_non_static_event_inline() {
    let length = Rc::new(Cell::new(0));
    let output = Rc::clone(&length);
    let handler = EventHandler::new(move |value: usize| output.set(value));
    let sink = EventSink::from(&handler).contramap_borrow(str::len);
    let owned = String::from("temporary");

    let subscription = BorrowingSource { value: &owned }
        .connect(sink)
        .inline()
        .expect("the Source is available");

    assert_eq!(length.get(), 9);
    drop(subscription);
}

#[test]
fn a_sink_preserves_its_borrowed_dispatch_type_until_inline_execution() {
    let length = Rc::new(Cell::new(0));
    let owned = String::from("temporary");

    let subscription = BorrowingSource { value: &owned }
        .connect(BorrowedLengthSink {
            output: Rc::clone(&length),
        })
        .inline()
        .expect("the Source is available");

    assert_eq!(length.get(), 9);
    drop(subscription);
}

#[test]
fn a_dispatcher_accepts_a_sink_specific_dispatch_type() {
    let mut dispatcher = FifoDispatcher::new();
    let output = Rc::new(Cell::new(0));
    let (provider, source) = channel::<u32>();

    let subscription = source
        .cloned()
        .connect(OwnedSink {
            output: Rc::clone(&output),
        })
        .via(&dispatcher)
        .expect("the Source is available");

    assert!(EventSink::from(&provider).send_inline(7).is_ok());
    assert_eq!(output.get(), 0);
    assert!(dispatcher.deliver_next());
    assert_eq!(output.get(), 7);
    drop(subscription);
}
