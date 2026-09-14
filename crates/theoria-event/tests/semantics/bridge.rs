use std::{cell::RefCell, rc::Rc, thread};

use theoria_event::{
    BridgeDispatch, BridgeProgress, BridgeSink, Dispatch, DispatchOutcome, Dispatcher,
    EventHandler, EventSink, EventSource, EventTarget, FifoDispatcher, OwningSink, bridge,
};

fn assert_send_sync<T: Send + Sync>() {}

fn assert_send<T: Send>() {}

#[test]
fn a_bridge_sink_preserves_cross_thread_dispatch_capabilities() {
    fn send<S>(sink: &S, event: u32) -> S::Dispatch
    where
        S: EventSink<u32>,
    {
        sink.send(event)
    }

    assert_send_sync::<BridgeSink<u32>>();
    assert_send::<BridgeDispatch<u32>>();

    let (bridge, sink) = bridge();
    assert_eq!(send(&sink, 7).run(), DispatchOutcome::Delivered);
    assert_eq!(bridge.forward_next(), BridgeProgress::Forwarded);
}

#[test]
fn a_value_crosses_a_thread_boundary_into_a_local_source() {
    let (bridge, sender) = bridge();
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let handler = EventHandler::new(move |value: u32| output.borrow_mut().push(value));
    let sink = EventTarget::from(&handler);
    bridge
        .source()
        .connect(OwningSink::new(sink))
        .inline()
        .expect("live bridge")
        .detach();

    thread::spawn(move || sender.send(7_u32).run())
        .join()
        .expect("the worker finishes");

    assert_eq!(bridge.forward_next(), BridgeProgress::Forwarded);
    assert_eq!(&*values.borrow(), &[7]);
    assert_eq!(bridge.forward_next(), BridgeProgress::Closed);
}

#[test]
fn progress_distinguishes_idle_from_closed() {
    let (bridge, sender) = bridge::<u32>();

    assert_eq!(bridge.forward_next(), BridgeProgress::Idle);
    assert_eq!(sender.send(1).run(), DispatchOutcome::Delivered);
    assert_eq!(sender.send(2).run(), DispatchOutcome::Delivered);
    drop(sender);

    assert_eq!(bridge.forward_next(), BridgeProgress::Forwarded);
    assert_eq!(bridge.forward_next(), BridgeProgress::Forwarded);
    assert_eq!(bridge.forward_next(), BridgeProgress::Closed);
}

#[test]
fn cloned_senders_keep_the_bridge_input_open() {
    let (bridge, first) = bridge::<u32>();
    let second = first.clone();
    drop(first);

    assert_eq!(bridge.forward_next(), BridgeProgress::Idle);
    assert_eq!(second.send(7).run(), DispatchOutcome::Delivered);
    assert_eq!(bridge.forward_next(), BridgeProgress::Forwarded);
    drop(second);
    assert_eq!(bridge.forward_next(), BridgeProgress::Closed);
}

#[test]
fn a_bridge_forwards_events_in_receive_order() {
    let (bridge, sender) = bridge::<u32>();
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let handler = EventHandler::new(move |value: u32| output.borrow_mut().push(value));
    bridge
        .source()
        .connect(OwningSink::new(EventTarget::from(&handler)))
        .inline()
        .expect("live bridge")
        .detach();

    assert_eq!(sender.send(1).run(), DispatchOutcome::Delivered);
    assert_eq!(sender.send(2).run(), DispatchOutcome::Delivered);
    assert_eq!(bridge.forward_next(), BridgeProgress::Forwarded);
    assert_eq!(bridge.forward_next(), BridgeProgress::Forwarded);

    assert_eq!(&*values.borrow(), &[1, 2]);
}

#[test]
fn a_bridge_dispatch_can_return_an_event_when_its_sink_is_gone() {
    let (bridge, sender) = bridge();
    drop(bridge);

    let event = String::from("event");
    assert_eq!(sender.send(event).try_run(), Err(String::from("event")));
}

#[test]
fn each_subscriber_selects_its_own_dispatcher() {
    let (bridge, sender) = bridge();
    let mut dispatcher = FifoDispatcher::new();
    let event_dispatcher: Dispatcher = dispatcher.as_ref().clone();
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let handler = EventHandler::new(move |value: u32| output.borrow_mut().push(value));
    let sink = EventTarget::from(&handler);
    bridge
        .source()
        .connect(OwningSink::new(sink))
        .via(event_dispatcher.clone())
        .expect("live bridge")
        .detach();

    assert_eq!(sender.send(7_u32).run(), DispatchOutcome::Delivered);
    assert_eq!(bridge.forward_next(), BridgeProgress::Forwarded);
    assert!(values.borrow().is_empty());

    assert!(dispatcher.deliver_next());
    assert_eq!(&*values.borrow(), &[7]);
}
