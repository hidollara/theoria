use std::{cell::RefCell, rc::Rc, thread};

use super::{TestSink, TestSinkOwner};
use theoria_event::{
    BridgeDispatch, BridgeProgress, BridgeSink, Dispatch, DispatchOutcome, DispatchQueue,
    EventBridge, EventSink, EventSource, Subscription,
};

fn assert_send_sync<T: Send + Sync>() {}

fn assert_send<T: Send>() {}

#[test]
fn a_bridge_sink_preserves_cross_thread_dispatch_capabilities() {
    fn send<'sink, S>(sink: &'sink S, event: u32) -> <&'sink S as EventSink<u32>>::Dispatch
    where
        &'sink S: EventSink<u32>,
    {
        EventSink::send(sink, event)
    }

    assert_send_sync::<BridgeSink<u32>>();
    assert_send::<BridgeDispatch<u32>>();

    let bridge = EventBridge::new();
    let sink = bridge.sink();
    assert_eq!(send(&sink, 7).run(), DispatchOutcome::Completed);
    assert_eq!(bridge.forward_next(), BridgeProgress::Forwarded);
}

#[test]
fn a_value_crosses_a_thread_boundary_into_a_local_source() {
    let bridge = EventBridge::new();
    let sender = bridge.sink();
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let handler = TestSinkOwner::new(move |value: u32| output.borrow_mut().push(value));
    let sink = TestSink::from(&handler);
    Subscription::builder(bridge.source(), sink)
        .owned()
        .inline()
        .expect("live bridge")
        .detach();

    thread::spawn(move || sender.send(7_u32).run())
        .join()
        .expect("the worker finishes");

    assert_eq!(bridge.forward_next(), BridgeProgress::Forwarded);
    assert_eq!(&*values.borrow(), &[7]);
    assert_eq!(bridge.forward_next(), BridgeProgress::Idle);
}

#[test]
fn progress_distinguishes_forwarded_events_from_idle() {
    let bridge = EventBridge::<u32>::new();
    let sender = bridge.sink();

    assert_eq!(bridge.forward_next(), BridgeProgress::Idle);
    assert_eq!((&sender).send(1).run(), DispatchOutcome::Completed);
    assert_eq!((&sender).send(2).run(), DispatchOutcome::Completed);
    drop(sender);

    assert_eq!(bridge.forward_next(), BridgeProgress::Forwarded);
    assert_eq!(bridge.forward_next(), BridgeProgress::Forwarded);
    assert_eq!(bridge.forward_next(), BridgeProgress::Idle);
}

#[test]
fn cloned_sinks_can_send_independently() {
    let bridge = EventBridge::<u32>::new();
    let first = bridge.sink();
    let second = first.clone();
    drop(first);

    assert_eq!(bridge.forward_next(), BridgeProgress::Idle);
    assert_eq!((&second).send(7).run(), DispatchOutcome::Completed);
    assert_eq!(bridge.forward_next(), BridgeProgress::Forwarded);
    drop(second);
    assert_eq!(bridge.forward_next(), BridgeProgress::Idle);
}

#[test]
fn a_bridge_can_issue_a_new_sink_after_previous_sinks_are_dropped() {
    let bridge = EventBridge::<u32>::new();
    drop(bridge.sink());

    assert_eq!(bridge.forward_next(), BridgeProgress::Idle);
    assert_eq!(bridge.sink().send(7).run(), DispatchOutcome::Completed);
    assert_eq!(bridge.forward_next(), BridgeProgress::Forwarded);
}

#[test]
fn a_bridge_forwards_events_in_receive_order() {
    let bridge = EventBridge::<u32>::new();
    let sender = bridge.sink();
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let handler = TestSinkOwner::new(move |value: u32| output.borrow_mut().push(value));
    Subscription::builder(bridge.source(), TestSink::from(&handler))
        .owned()
        .inline()
        .expect("live bridge")
        .detach();

    assert_eq!((&sender).send(1).run(), DispatchOutcome::Completed);
    assert_eq!((&sender).send(2).run(), DispatchOutcome::Completed);
    assert_eq!(bridge.forward_next(), BridgeProgress::Forwarded);
    assert_eq!(bridge.forward_next(), BridgeProgress::Forwarded);

    assert_eq!(&*values.borrow(), &[1, 2]);
}

#[test]
fn a_bridge_dispatch_can_return_an_event_when_its_bridge_is_gone() {
    let bridge = EventBridge::new();
    let sender = bridge.sink();
    let source = bridge.source();
    drop(bridge);

    let event = String::from("event");
    assert_eq!(sender.send(event).try_run(), Err(String::from("event")));
    assert!(source.register(|_| {}).is_none());
}

#[test]
fn each_subscriber_selects_its_own_dispatcher() {
    let bridge = EventBridge::new();
    let sender = bridge.sink();
    let mut queue = DispatchQueue::new();
    let event_dispatcher = queue.dispatcher();
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let handler = TestSinkOwner::new(move |value: u32| output.borrow_mut().push(value));
    let sink = TestSink::from(&handler);
    Subscription::builder(bridge.source(), sink)
        .owned()
        .via(event_dispatcher.clone())
        .expect("live bridge")
        .detach();

    assert_eq!(sender.send(7_u32).run(), DispatchOutcome::Completed);
    assert_eq!(bridge.forward_next(), BridgeProgress::Forwarded);
    assert!(values.borrow().is_empty());

    assert!(queue.run_next().is_some());
    assert_eq!(&*values.borrow(), &[7]);
}
