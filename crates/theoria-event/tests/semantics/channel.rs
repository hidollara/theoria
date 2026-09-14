use std::{cell::RefCell, rc::Rc, sync::Arc};

use super::{TestSink, TestSinkOwner, drain};
use theoria_event::{
    Channel, Dispatch, DispatchOutcome, DispatchQueue, EventSink, EventSource, Subscription,
};

#[test]
fn a_borrowed_source_runs_without_owning_its_publication() {
    struct Publication(String);

    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let handler = TestSinkOwner::new(move |value: usize| output.borrow_mut().push(value));
    let sink = TestSink::from(&handler).contramap(|event: &Publication| event.0.len());
    let channel = Channel::<Publication>::new();
    let source = channel.source();
    Subscription::builder(source, sink)
        .inline()
        .expect("live Channel")
        .detach();

    channel
        .sink()
        .send(Publication(String::from("event")))
        .run();

    assert_eq!(&*values.borrow(), &[5]);
}

#[test]
fn an_owned_connection_can_defer_a_borrowed_publication() {
    let mut queue = DispatchQueue::new();
    let dispatcher = queue.dispatcher();
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let handler = TestSinkOwner::new(move |value: u32| output.borrow_mut().push(value));
    let channel = Channel::<u32>::new();
    let source = channel.source();
    Subscription::builder(source, TestSink::from(&handler))
        .owned()
        .via(dispatcher.clone())
        .expect("live Channel")
        .detach();

    channel.sink().send(7).run();
    assert!(values.borrow().is_empty());
    assert!(queue.run_next().is_some());
    assert_eq!(&*values.borrow(), &[7]);
}

#[test]
fn an_rc_channel_defers_move_only_values_without_cloning_the_value() {
    struct MoveOnly(String);

    let mut queue = DispatchQueue::new();
    let dispatcher = queue.dispatcher();
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let handler = TestSinkOwner::new(move |value: usize| output.borrow_mut().push(value));
    let sink = TestSink::from(&handler).contramap(|event: Rc<MoveOnly>| event.0.len());
    let channel = Channel::<Rc<MoveOnly>>::new();
    let source = channel.source();
    Subscription::builder(source, sink)
        .owned()
        .via(dispatcher.clone())
        .expect("live Channel")
        .detach();

    channel
        .sink()
        .send(Rc::new(MoveOnly(String::from("shared"))))
        .run();
    assert!(queue.run_next().is_some());
    assert_eq!(&*values.borrow(), &[6]);
}

#[test]
fn an_arc_channel_uses_the_same_deferred_source_contract() {
    struct MoveOnly(String);

    let mut queue = DispatchQueue::new();
    let dispatcher = queue.dispatcher();
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let handler = TestSinkOwner::new(move |value: usize| output.borrow_mut().push(value));
    let sink = TestSink::from(&handler).contramap(|event: Arc<MoveOnly>| event.0.len());
    let channel = Channel::<Arc<MoveOnly>>::new();
    let source = channel.source();
    Subscription::builder(source, sink)
        .owned()
        .via(dispatcher.clone())
        .expect("live Channel")
        .detach();

    channel
        .sink()
        .send(Arc::new(MoveOnly(String::from("shared"))))
        .run();
    assert!(queue.run_next().is_some());
    assert_eq!(&*values.borrow(), &[6]);
}

#[test]
fn dropping_a_subscription_stops_future_publications() {
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let handler = TestSinkOwner::new(move |value: u32| output.borrow_mut().push(value));
    let channel = Channel::<u32>::new();
    let source = channel.source();
    let subscription = Subscription::builder(source, TestSink::from(&handler))
        .owned()
        .inline()
        .expect("live Channel");

    drop(subscription);
    channel.sink().send(1).run();

    assert!(values.borrow().is_empty());
}

#[test]
fn unsubscribing_stops_future_publications() {
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let handler = TestSinkOwner::new(move |value: u32| output.borrow_mut().push(value));
    let channel = Channel::<u32>::new();
    let source = channel.source();
    let subscription = Subscription::builder(source, TestSink::from(&handler))
        .owned()
        .inline()
        .expect("live Channel");

    subscription.unsubscribe();
    channel.sink().send(1).run();

    assert!(values.borrow().is_empty());
}

#[test]
fn detaching_leaves_the_subscription_owned_by_the_channel() {
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let handler = TestSinkOwner::new(move |value: u32| output.borrow_mut().push(value));
    let channel = Channel::<u32>::new();
    let source = channel.source();
    Subscription::builder(source, TestSink::from(&handler))
        .owned()
        .inline()
        .expect("live Channel")
        .detach();

    channel.sink().send(1).run();

    assert_eq!(&*values.borrow(), &[1]);
}

#[test]
fn calling_a_source_unregister_operation_stops_future_publications() {
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let channel = Channel::<u32>::new();
    let source = channel.source();
    let unregister = source
        .register(move |value| output.borrow_mut().push(*value))
        .expect("live Channel");

    channel.sink().send(1).run();
    unregister();
    channel.sink().send(2).run();

    assert_eq!(&*values.borrow(), &[1]);
}

#[test]
fn dropping_a_source_unregister_operation_leaves_the_registration_intact() {
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let channel = Channel::<u32>::new();
    let source = channel.source();
    let unregister = source
        .register(move |value| output.borrow_mut().push(*value))
        .expect("live Channel");

    drop(unregister);
    channel.sink().send(1).run();

    assert_eq!(&*values.borrow(), &[1]);
}

#[test]
fn source_and_sink_do_not_keep_their_channel_alive() {
    let channel = Channel::<u32>::new();
    let source = channel.source();
    let input = channel.sink();
    let pending = input.send(1);
    drop(channel);

    assert!(source.register(|_| {}).is_none());
    assert_eq!(pending.run(), DispatchOutcome::SinkClosed);
}

#[test]
fn subscription_changes_during_publication_apply_to_the_next_event() {
    let channel = Channel::<u32>::new();
    let source = channel.source();
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let late_source = source.clone();
    let late_values = Rc::clone(&values);
    let late_handler = Rc::new(TestSinkOwner::new(move |value: u32| {
        late_values.borrow_mut().push(("late", value));
    }));
    let late_subscription = Rc::new(RefCell::new(None::<Subscription>));
    let install_once = Rc::clone(&late_subscription);
    let keep_handler = Rc::clone(&late_handler);
    let unregister_first = source
        .register(move |value| {
            output.borrow_mut().push(("first", *value));
            if install_once.borrow().is_none() {
                let subscription = Subscription::builder(
                    late_source.clone(),
                    TestSink::from(keep_handler.as_ref()),
                )
                .owned()
                .inline()
                .expect("live Channel");
                install_once.replace(Some(subscription));
            }
        })
        .expect("live Channel");

    channel.sink().send(1).run();
    channel.sink().send(2).run();
    unregister_first();

    assert_eq!(
        &*values.borrow(),
        &[("first", 1), ("first", 2), ("late", 2)]
    );
}

#[test]
fn subscribers_are_invoked_in_registration_order() {
    let channel = Channel::<()>::new();
    let source = channel.source();
    let order = Rc::new(RefCell::new(Vec::new()));
    let first_order = Rc::clone(&order);
    let unregister_first = source
        .clone()
        .register(move |_| first_order.borrow_mut().push(1))
        .expect("live Channel");
    let second_order = Rc::clone(&order);
    let unregister_second = source
        .register(move |_| second_order.borrow_mut().push(2))
        .expect("live Channel");

    channel.sink().send(()).run();
    unregister_first();
    unregister_second();

    assert_eq!(&*order.borrow(), &[1, 2]);
}

#[test]
fn unsubscribing_does_not_cancel_an_already_deferred_dispatch() {
    let mut queue = DispatchQueue::new();
    let dispatcher = queue.dispatcher();
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let handler = TestSinkOwner::new(move |value: u32| output.borrow_mut().push(value));
    let channel = Channel::<u32>::new();
    let source = channel.source();
    let subscription = Subscription::builder(source, TestSink::from(&handler))
        .owned()
        .via(dispatcher.clone())
        .expect("live Channel");

    channel.sink().send(1).run();
    subscription.unsubscribe();
    drain(&mut queue);

    assert_eq!(&*values.borrow(), &[1]);
}
