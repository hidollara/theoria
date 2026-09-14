use std::{cell::Cell, rc::Rc};

use theoria_event::{EventHandler, EventSink, EventSource, Subscription};

struct BorrowingSource<'value> {
    value: &'value str,
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
