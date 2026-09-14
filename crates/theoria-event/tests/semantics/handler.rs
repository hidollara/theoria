use std::{
    cell::Cell,
    panic::{AssertUnwindSafe, catch_unwind},
    rc::Rc,
};

use super::drain;
use theoria_event::{
    Dispatch, DispatchOutcome, DispatchQueue, Dispatcher, EventHandler, EventTarget,
};

#[test]
fn direct_delivery_returns_events_after_handler_drop() {
    let calls = Rc::new(Cell::new(0));
    let handler_calls = Rc::clone(&calls);
    let handler = EventHandler::new(move |_: u32| handler_calls.set(handler_calls.get() + 1));
    let first = EventTarget::from(&handler);
    let second = EventTarget::from(&handler);

    assert_eq!(first.send(1_u32).run(), DispatchOutcome::Completed);
    assert_eq!(second.send(2_u32).run(), DispatchOutcome::Completed);
    assert_eq!(calls.get(), 2);
    drop(handler);
    assert_eq!(first.send(3_u32).run(), DispatchOutcome::SinkClosed);
    assert_eq!(second.send(4_u32).run(), DispatchOutcome::SinkClosed);
    assert_eq!(calls.get(), 2);
}

#[test]
fn a_panicking_handler_is_restored_for_a_later_dispatch() {
    let mut queue = DispatchQueue::new();
    let event_dispatcher = queue.dispatcher();
    let calls = Rc::new(Cell::new(0));
    let handler_calls = Rc::clone(&calls);
    let handler = EventHandler::new(move |(): ()| {
        handler_calls.set(handler_calls.get() + 1);
        if handler_calls.get() == 1 {
            panic!("first Dispatch");
        }
    });
    let sink = EventTarget::from(&handler);

    assert!(event_dispatcher.submit(sink.send(())).is_ok());
    let first = catch_unwind(AssertUnwindSafe(|| {
        drain(&mut queue);
    }));
    assert!(first.is_err());
    assert!(event_dispatcher.submit(sink.send(())).is_ok());
    drain(&mut queue);
    assert_eq!(calls.get(), 2);
}

#[test]
fn recursively_dispatching_to_the_same_handler_panics() {
    let recurse = Rc::new(Cell::new(true));
    let should_recurse = Rc::clone(&recurse);
    let sink_slot = Rc::new(std::cell::RefCell::new(None::<EventTarget<()>>));
    let nested_sink = Rc::clone(&sink_slot);
    let calls = Rc::new(Cell::new(0));
    let observed_calls = Rc::clone(&calls);
    let handler = EventHandler::new(move |(): ()| {
        observed_calls.set(observed_calls.get() + 1);
        if should_recurse.replace(false) {
            let sink = nested_sink
                .borrow()
                .as_ref()
                .expect("the recursive event target is installed")
                .clone();
            let _ = sink.send(()).run();
        }
    });
    let sink = EventTarget::from(&handler);
    sink_slot.replace(Some(sink.clone()));

    let result = catch_unwind(AssertUnwindSafe(|| {
        let _ = sink.send(()).run();
    }));
    assert!(result.is_err());

    assert_eq!(sink.send(()).run(), DispatchOutcome::Completed);
    assert_eq!(calls.get(), 2);
}

#[test]
fn dropping_a_handler_prevents_a_queued_dispatch_from_invoking_it() {
    let mut queue = DispatchQueue::new();
    let event_dispatcher = queue.dispatcher();
    let calls = Rc::new(Cell::new(0));
    let handler_calls = Rc::clone(&calls);
    let handler = EventHandler::new(move |(): ()| {
        handler_calls.set(handler_calls.get() + 1);
    });
    let sink = EventTarget::from(&handler);

    assert!(event_dispatcher.submit(sink.send(())).is_ok());
    drop(handler);
    assert!(queue.run_next().is_some());
    assert!(queue.run_next().is_none());

    assert_eq!(calls.get(), 0);
    assert!(event_dispatcher.submit(sink.send(())).is_ok());
    assert!(queue.run_next().is_some());
    assert_eq!(calls.get(), 0);
}
