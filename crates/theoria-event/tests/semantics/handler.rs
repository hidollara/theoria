use std::{
    cell::Cell,
    panic::{AssertUnwindSafe, catch_unwind},
    rc::Rc,
};

use super::drain;
use theoria_event::{Dispatcher, EventHandler, EventSink, FifoDispatcher};

#[test]
fn direct_delivery_returns_events_after_handler_drop() {
    let calls = Rc::new(Cell::new(0));
    let handler_calls = Rc::clone(&calls);
    let handler = EventHandler::new(move |_: u32| handler_calls.set(handler_calls.get() + 1));
    let first = EventSink::from(&handler);
    let second = EventSink::from(&handler);

    assert!(first.send(1_u32).is_ok());
    assert!(second.send(2_u32).is_ok());
    assert_eq!(calls.get(), 2);
    drop(handler);
    assert_eq!(first.send(3_u32), Err(3_u32));
    assert_eq!(second.send(4_u32), Err(4_u32));
    assert_eq!(calls.get(), 2);
}

#[test]
fn a_panicking_handler_is_restored_for_a_later_dispatch() {
    let mut dispatcher = FifoDispatcher::new();
    let event_dispatcher: Dispatcher = dispatcher.as_ref().clone();
    let calls = Rc::new(Cell::new(0));
    let handler_calls = Rc::clone(&calls);
    let handler = EventHandler::new(move |(): ()| {
        handler_calls.set(handler_calls.get() + 1);
        if handler_calls.get() == 1 {
            panic!("first Dispatch");
        }
    });
    let sink = EventSink::from(&handler);

    assert!(event_dispatcher.send(&sink, ()).is_ok());
    let first = catch_unwind(AssertUnwindSafe(|| {
        drain(&mut dispatcher);
    }));
    assert!(first.is_err());
    assert!(event_dispatcher.send(&sink, ()).is_ok());
    drain(&mut dispatcher);
    assert_eq!(calls.get(), 2);
}

#[test]
fn recursively_dispatching_to_the_same_handler_panics() {
    let recurse = Rc::new(Cell::new(true));
    let should_recurse = Rc::clone(&recurse);
    let sink_slot = Rc::new(std::cell::RefCell::new(None::<EventSink<()>>));
    let nested_sink = Rc::clone(&sink_slot);
    let calls = Rc::new(Cell::new(0));
    let observed_calls = Rc::clone(&calls);
    let handler = EventHandler::new(move |(): ()| {
        observed_calls.set(observed_calls.get() + 1);
        if should_recurse.replace(false) {
            let sink = nested_sink
                .borrow()
                .as_ref()
                .expect("the recursive Sink is installed")
                .clone();
            sink.send(()).expect("the recursive Sink remains available");
        }
    });
    let sink = EventSink::from(&handler);
    sink_slot.replace(Some(sink.clone()));

    let result = catch_unwind(AssertUnwindSafe(|| {
        let _ = sink.send(());
    }));
    assert!(result.is_err());

    assert!(sink.send(()).is_ok());
    assert_eq!(calls.get(), 2);
}

#[test]
fn dropping_a_handler_prevents_a_queued_dispatch_from_invoking_it() {
    let mut dispatcher = FifoDispatcher::new();
    let event_dispatcher: Dispatcher = dispatcher.as_ref().clone();
    let calls = Rc::new(Cell::new(0));
    let handler_calls = Rc::clone(&calls);
    let handler = EventHandler::new(move |(): ()| {
        handler_calls.set(handler_calls.get() + 1);
    });
    let sink = EventSink::from(&handler);

    assert!(event_dispatcher.send(&sink, ()).is_ok());
    drop(handler);
    assert!(dispatcher.deliver_next());
    assert!(!dispatcher.deliver_next());

    assert_eq!(calls.get(), 0);
    assert!(event_dispatcher.send(&sink, ()).is_ok());
    assert!(dispatcher.deliver_next());
    assert_eq!(calls.get(), 0);
}
