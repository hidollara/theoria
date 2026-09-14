use std::{cell::RefCell, rc::Rc, sync::Arc};

use super::drain;
use theoria_event::{
    DispatchOutcome, Dispatcher, EventHandler, EventSink, EventSource, FifoDispatcher,
    Subscription, channel,
};

#[test]
fn a_borrowed_source_runs_without_owning_its_publication() {
    struct Publication(String);

    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let handler = EventHandler::new(move |value: usize| output.borrow_mut().push(value));
    let sink = EventSink::from(&handler).contramap_borrow(|event: &Publication| event.0.len());
    let (provider, source) = channel::<Publication>();
    source
        .borrowed()
        .connect(sink)
        .inline()
        .expect("live Provider")
        .detach();

    EventSink::from(&provider)
        .send(Publication(String::from("event")))
        .run();

    assert_eq!(&*values.borrow(), &[5]);
}

#[test]
fn a_cloned_source_can_defer_its_dispatches() {
    let mut fifo = FifoDispatcher::new();
    let dispatcher: Dispatcher = fifo.as_ref().clone();
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let handler = EventHandler::new(move |value: u32| output.borrow_mut().push(value));
    let (provider, source) = channel::<u32>();
    source
        .cloned()
        .connect(EventSink::from(&handler))
        .via(dispatcher.clone())
        .expect("live Provider")
        .detach();

    EventSink::from(&provider).send(7).run();
    assert!(values.borrow().is_empty());
    assert!(fifo.deliver_next());
    assert_eq!(&*values.borrow(), &[7]);
}

#[test]
fn an_rc_channel_defers_move_only_values_without_cloning_the_value() {
    struct MoveOnly(String);

    let mut fifo = FifoDispatcher::new();
    let dispatcher: Dispatcher = fifo.as_ref().clone();
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let handler = EventHandler::new(move |value: usize| output.borrow_mut().push(value));
    let sink = EventSink::from(&handler).contramap(|event: Rc<MoveOnly>| event.0.len());
    let (provider, source) = channel::<Rc<MoveOnly>>();
    source
        .cloned()
        .connect(sink)
        .via(dispatcher.clone())
        .expect("live Provider")
        .detach();

    EventSink::from(&provider)
        .send(Rc::new(MoveOnly(String::from("shared"))))
        .run();
    assert!(fifo.deliver_next());
    assert_eq!(&*values.borrow(), &[6]);
}

#[test]
fn an_arc_channel_uses_the_same_deferred_source_contract() {
    struct MoveOnly(String);

    let mut fifo = FifoDispatcher::new();
    let dispatcher: Dispatcher = fifo.as_ref().clone();
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let handler = EventHandler::new(move |value: usize| output.borrow_mut().push(value));
    let sink = EventSink::from(&handler).contramap(|event: Arc<MoveOnly>| event.0.len());
    let (provider, source) = channel::<Arc<MoveOnly>>();
    source
        .cloned()
        .connect(sink)
        .via(dispatcher.clone())
        .expect("live Provider")
        .detach();

    EventSink::from(&provider)
        .send(Arc::new(MoveOnly(String::from("shared"))))
        .run();
    assert!(fifo.deliver_next());
    assert_eq!(&*values.borrow(), &[6]);
}

#[test]
fn dropping_a_subscription_stops_future_publications() {
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let handler = EventHandler::new(move |value: u32| output.borrow_mut().push(value));
    let (provider, source) = channel::<u32>();
    let subscription = source
        .cloned()
        .connect(EventSink::from(&handler))
        .inline()
        .expect("live Provider");

    drop(subscription);
    EventSink::from(&provider).send(1).run();

    assert!(values.borrow().is_empty());
}

#[test]
fn unsubscribing_stops_future_publications() {
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let handler = EventHandler::new(move |value: u32| output.borrow_mut().push(value));
    let (provider, source) = channel::<u32>();
    let subscription = source
        .cloned()
        .connect(EventSink::from(&handler))
        .inline()
        .expect("live Provider");

    subscription.unsubscribe();
    EventSink::from(&provider).send(1).run();

    assert!(values.borrow().is_empty());
}

#[test]
fn detaching_leaves_the_subscription_owned_by_the_provider() {
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let handler = EventHandler::new(move |value: u32| output.borrow_mut().push(value));
    let (provider, source) = channel::<u32>();
    source
        .cloned()
        .connect(EventSink::from(&handler))
        .inline()
        .expect("live Provider")
        .detach();

    EventSink::from(&provider).send(1).run();

    assert_eq!(&*values.borrow(), &[1]);
}

#[test]
fn source_and_input_do_not_keep_their_provider_alive() {
    let (provider, source) = channel::<u32>();
    let input = EventSink::from(&provider);
    drop(provider);

    assert!(source.cloned().subscribe(|_| {}).is_none());
    assert_eq!(input.send(1).run(), DispatchOutcome::SinkUnavailable);
}

#[test]
fn subscription_changes_during_publication_apply_to_the_next_event() {
    let (provider, source) = channel::<u32>();
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let late_source = source.clone();
    let late_values = Rc::clone(&values);
    let late_handler = Rc::new(EventHandler::new(move |value: u32| {
        late_values.borrow_mut().push(("late", value));
    }));
    let late_subscription = Rc::new(RefCell::new(None::<Subscription>));
    let install_once = Rc::clone(&late_subscription);
    let keep_handler = Rc::clone(&late_handler);
    let first = source
        .cloned()
        .subscribe(move |value| {
            output.borrow_mut().push(("first", value));
            if install_once.borrow().is_none() {
                let subscription = late_source
                    .cloned()
                    .connect(EventSink::from(keep_handler.as_ref()))
                    .inline()
                    .expect("live Provider");
                install_once.replace(Some(subscription));
            }
        })
        .expect("live Provider");

    EventSink::from(&provider).send(1).run();
    EventSink::from(&provider).send(2).run();
    drop(first);

    assert_eq!(
        &*values.borrow(),
        &[("first", 1), ("first", 2), ("late", 2)]
    );
}

#[test]
fn subscribers_are_invoked_in_registration_order() {
    let (provider, source) = channel::<()>();
    let order = Rc::new(RefCell::new(Vec::new()));
    let first_order = Rc::clone(&order);
    let first = source
        .cloned()
        .subscribe(move |_| first_order.borrow_mut().push(1))
        .expect("live Provider");
    let second_order = Rc::clone(&order);
    let second = source
        .cloned()
        .subscribe(move |_| second_order.borrow_mut().push(2))
        .expect("live Provider");

    EventSink::from(&provider).send(()).run();
    drop((first, second));

    assert_eq!(&*order.borrow(), &[1, 2]);
}

#[test]
fn unsubscribing_does_not_cancel_an_already_deferred_dispatch() {
    let mut fifo = FifoDispatcher::new();
    let dispatcher: Dispatcher = fifo.as_ref().clone();
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let handler = EventHandler::new(move |value: u32| output.borrow_mut().push(value));
    let (provider, source) = channel::<u32>();
    let subscription = source
        .cloned()
        .connect(EventSink::from(&handler))
        .via(dispatcher.clone())
        .expect("live Provider");

    EventSink::from(&provider).send(1).run();
    subscription.unsubscribe();
    drain(&mut fifo);

    assert_eq!(&*values.borrow(), &[1]);
}
