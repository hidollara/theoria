use std::{cell::RefCell, rc::Rc, thread};

use theoria_event::{
    BridgeProgress, Dispatcher, EventHandler, EventSink, EventSource, FifoDispatcher, bridge,
};

#[test]
fn a_value_crosses_a_thread_boundary_into_a_local_source() {
    let (bridge, sender) = bridge();
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let handler = EventHandler::new(move |value: u32| output.borrow_mut().push(value));
    let sink = EventSink::from(&handler);
    bridge
        .source()
        .cloned()
        .connect(sink)
        .inline()
        .expect("live bridge")
        .detach();

    thread::spawn(move || sender.send(7_u32).expect("the Bridge remains alive"))
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
    sender.send(1).expect("the Bridge remains alive");
    sender.send(2).expect("the Bridge remains alive");
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
    second.send(7).expect("the cloned Sender remains alive");
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
        .cloned()
        .connect(EventSink::from(&handler))
        .inline()
        .expect("live bridge")
        .detach();

    sender.send(1).expect("the Bridge remains alive");
    sender.send(2).expect("the Bridge remains alive");
    assert_eq!(bridge.forward_next(), BridgeProgress::Forwarded);
    assert_eq!(bridge.forward_next(), BridgeProgress::Forwarded);

    assert_eq!(&*values.borrow(), &[1, 2]);
}

#[test]
fn a_failed_send_returns_ownership_of_the_event() {
    let (bridge, sender) = bridge();
    drop(bridge);

    let event = String::from("still owned");
    assert_eq!(sender.send(event), Err(String::from("still owned")));
}

#[test]
fn each_subscriber_selects_its_own_dispatcher() {
    let (bridge, sender) = bridge();
    let mut dispatcher = FifoDispatcher::new();
    let event_dispatcher: Dispatcher = dispatcher.as_ref().clone();
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let handler = EventHandler::new(move |value: u32| output.borrow_mut().push(value));
    let sink = EventSink::from(&handler);
    bridge
        .source()
        .cloned()
        .connect(sink)
        .via(&event_dispatcher)
        .expect("live bridge")
        .detach();

    sender.send(7_u32).expect("the Bridge remains alive");
    assert_eq!(bridge.forward_next(), BridgeProgress::Forwarded);
    assert!(values.borrow().is_empty());

    assert!(dispatcher.deliver_next());
    assert_eq!(&*values.borrow(), &[7]);
}
