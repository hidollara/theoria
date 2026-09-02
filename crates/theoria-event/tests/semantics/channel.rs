use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use super::drain;
use theoria_event::{
    DispatchRoute, EventDispatch, EventDispatcher, EventHandler, EventSink, EventSubscription,
    FifoDispatcher, SubmissionOutcome, channel,
};

struct RejectOnceDispatcher {
    rejected: Cell<bool>,
    pending: RefCell<Vec<EventDispatch>>,
}

#[test]
fn subscription_mapping_runs_when_its_dispatch_executes() {
    struct MoveOnly(String);

    let mut dispatcher = FifoDispatcher::new();
    let route = dispatcher.route();
    let mappings = Rc::new(Cell::new(0));
    let mapped = Rc::clone(&mappings);
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let handler = EventHandler::new(move |value: usize| output.borrow_mut().push(value));
    let sink = EventSink::from(&handler);
    let (provider, source) = channel::<MoveOnly>();
    source
        .subscribe_map(&sink, &route, move |event| {
            mapped.set(mapped.get() + 1);
            event.0.len()
        })
        .expect("live provider")
        .detach();

    assert!(route.send(&provider, MoveOnly("letter".into())).is_ok());
    assert!(dispatcher.deliver_next());
    assert_eq!(
        mappings.get(),
        0,
        "publication expansion must not run subscription adapters"
    );
    assert!(dispatcher.deliver_next());
    assert_eq!(mappings.get(), 1);
    assert_eq!(&*values.borrow(), &[6]);
}

impl EventDispatcher for RejectOnceDispatcher {
    fn submit(&self, dispatch: EventDispatch) -> Result<SubmissionOutcome, EventDispatch> {
        if !self.rejected.replace(true) {
            return Err(dispatch);
        }
        self.pending.borrow_mut().push(dispatch);
        Ok(SubmissionOutcome::Deferred)
    }
}

#[test]
fn an_inline_subscription_dispatch_can_unsubscribe_during_publication() {
    let (provider, source) = channel::<u32>();
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let handler = EventHandler::new(move |value| output.borrow_mut().push(value));
    let sink = EventSink::from(&handler);
    let subscription = Rc::new(RefCell::new(None::<EventSubscription<u32>>));
    let callback_subscription = Rc::clone(&subscription);
    subscription.replace(Some(
        source
            .subscribe_map(&sink, &DispatchRoute::Inline, move |event| {
                callback_subscription
                    .borrow_mut()
                    .take()
                    .expect("subscription installed")
                    .unsubscribe();
                *event
            })
            .expect("live provider"),
    ));

    assert!(DispatchRoute::Inline.send(&provider, 1_u32).is_ok());
    assert!(DispatchRoute::Inline.send(&provider, 2_u32).is_ok());
    assert_eq!(&*values.borrow(), &[1]);
}

#[test]
fn source_and_input_do_not_keep_their_provider_alive() {
    let mut dispatcher = FifoDispatcher::new();
    let route = dispatcher.route();
    let (provider, source) = channel::<u32>();
    let input = EventSink::from(&provider);
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let handler = EventHandler::new(move |value: u32| output.borrow_mut().push(value));
    let sink = EventSink::from(&handler);
    source
        .subscribe_cloned(&sink, &route)
        .expect("live provider")
        .detach();

    assert!(route.send(&input, 1_u32).is_ok());
    drain(&mut dispatcher);
    assert_eq!(&*values.borrow(), &[1]);
    drop(provider);
    assert!(source.subscribe_cloned(&sink, &route).is_none());
    assert!(route.send(&input, 2_u32).is_ok());
    drain(&mut dispatcher);
    assert_eq!(&*values.borrow(), &[1]);
}

#[test]
fn cloned_source_views_observe_the_same_channel() {
    let (provider, first_source) = theoria_event::channel::<u32>();
    let second_source = first_source.clone();
    let values = Rc::new(RefCell::new(Vec::new()));
    let first_values = Rc::clone(&values);
    let first_handler = EventHandler::new(move |value: u32| {
        first_values.borrow_mut().push(("first", value));
    });
    let second_values = Rc::clone(&values);
    let second_handler = EventHandler::new(move |value: u32| {
        second_values.borrow_mut().push(("second", value));
    });
    first_source
        .subscribe_cloned(EventSink::from(&first_handler), &DispatchRoute::Inline)
        .expect("live provider")
        .detach();
    second_source
        .subscribe_cloned(EventSink::from(&second_handler), &DispatchRoute::Inline)
        .expect("live provider")
        .detach();

    assert!(DispatchRoute::Inline.send(&provider, 7_u32).is_ok());
    assert_eq!(&*values.borrow(), &[("first", 7), ("second", 7)]);
}

#[test]
fn subscription_changes_during_inline_fan_out_apply_to_the_next_event() {
    let (provider, source) = channel::<u32>();
    let source = Rc::new(source);
    let order = Rc::new(RefCell::new(Vec::new()));

    let first_order = Rc::clone(&order);
    let first_handler = EventHandler::new(move |value: u32| {
        first_order.borrow_mut().push(("first", value));
    });
    let second_order = Rc::clone(&order);
    let second_handler = EventHandler::new(move |value: u32| {
        second_order.borrow_mut().push(("second", value));
    });
    let third_order = Rc::clone(&order);
    let third_handler = EventHandler::new(move |value: u32| {
        third_order.borrow_mut().push(("third", value));
    });

    let second_subscription = Rc::new(RefCell::new(None::<EventSubscription<u32>>));
    let changed_second = Rc::clone(&second_subscription);
    let third_subscription = Rc::new(RefCell::new(None::<EventSubscription<u32>>));
    let changed_third = Rc::clone(&third_subscription);
    let changed_source = Rc::clone(&source);
    let third_sink = EventSink::from(&third_handler);
    source
        .subscribe_map(
            EventSink::from(&first_handler),
            &DispatchRoute::Inline,
            move |event| {
                if changed_third.borrow().is_none() {
                    changed_second
                        .borrow_mut()
                        .take()
                        .expect("second subscription installed")
                        .unsubscribe();
                    let subscription = changed_source
                        .subscribe_cloned(&third_sink, &DispatchRoute::Inline)
                        .expect("live provider");
                    changed_third.replace(Some(subscription));
                }
                *event
            },
        )
        .expect("live provider")
        .detach();
    second_subscription.replace(Some(
        source
            .subscribe_cloned(EventSink::from(&second_handler), &DispatchRoute::Inline)
            .expect("live provider"),
    ));

    assert!(DispatchRoute::Inline.send(&provider, 1_u32).is_ok());
    assert!(DispatchRoute::Inline.send(&provider, 2_u32).is_ok());
    assert_eq!(
        &*order.borrow(),
        &[("first", 1), ("second", 1), ("first", 2), ("third", 2)]
    );
}

#[test]
fn dropping_a_subscription_stops_future_publications() {
    let mut dispatcher = FifoDispatcher::new();
    let route = dispatcher.route();
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let handler = EventHandler::new(move |value: u32| output.borrow_mut().push(value));
    let sink = EventSink::from(&handler);
    let (provider, source) = channel::<u32>();
    let subscription = source
        .subscribe_cloned(&sink, &route)
        .expect("live provider");
    drop(subscription);

    assert!(route.send(&provider, 1_u32).is_ok());
    drain(&mut dispatcher);
    assert!(values.borrow().is_empty());
}

#[test]
fn explicitly_unsubscribing_stops_future_publications() {
    let mut dispatcher = FifoDispatcher::new();
    let route = dispatcher.route();
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let handler = EventHandler::new(move |value: u32| output.borrow_mut().push(value));
    let sink = EventSink::from(&handler);
    let (provider, source) = channel::<u32>();
    let subscription = source
        .subscribe_cloned(&sink, &route)
        .expect("live provider");

    subscription.unsubscribe();
    assert!(route.send(&provider, 1_u32).is_ok());
    drain(&mut dispatcher);

    assert!(values.borrow().is_empty());
}

#[test]
fn detaching_keeps_a_subscription_until_its_provider_is_dropped() {
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let handler = EventHandler::new(move |value: u32| output.borrow_mut().push(value));
    let (provider, source) = theoria_event::channel::<u32>();
    let input = EventSink::from(&provider);
    source
        .subscribe_cloned(EventSink::from(&handler), &DispatchRoute::Inline)
        .expect("live provider")
        .detach();

    assert!(DispatchRoute::Inline.send(&input, 1_u32).is_ok());
    assert_eq!(&*values.borrow(), &[1]);

    drop(provider);
    assert!(DispatchRoute::Inline.send(&input, 2_u32).is_ok());
    assert_eq!(&*values.borrow(), &[1]);
}

#[test]
fn publication_removes_subscriptions_with_expired_sinks() {
    let mut dispatcher = FifoDispatcher::new();
    let route = dispatcher.route();
    let mappings = Rc::new(Cell::new(0));
    let mapped = Rc::clone(&mappings);
    let handler = EventHandler::new(|_: u32| {});
    let sink = EventSink::from(&handler);
    let (provider, source) = channel::<u32>();
    source
        .subscribe_map(&sink, &route, move |event| {
            mapped.set(mapped.get() + 1);
            *event
        })
        .expect("live provider")
        .detach();
    drop(handler);

    assert!(route.send(&provider, 1_u32).is_ok());
    assert!(dispatcher.deliver_next());
    assert!(!dispatcher.deliver_next());

    assert!(route.send(&provider, 2_u32).is_ok());
    assert!(dispatcher.deliver_next());
    assert!(!dispatcher.deliver_next());
    assert_eq!(mappings.get(), 0);
}

#[test]
fn a_rejected_subscriber_submission_does_not_remove_the_subscription() {
    let dispatcher = Rc::new(RejectOnceDispatcher {
        rejected: Cell::new(false),
        pending: RefCell::new(Vec::new()),
    });
    let route = DispatchRoute::Dispatcher(dispatcher.clone());
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let handler = EventHandler::new(move |value: u32| output.borrow_mut().push(value));
    let sink = EventSink::from(&handler);
    let (provider, source) = channel::<u32>();
    source
        .subscribe_cloned(&sink, &route)
        .expect("live provider")
        .detach();

    assert!(DispatchRoute::Inline.send(&provider, 1_u32).is_ok());
    assert!(dispatcher.pending.borrow().is_empty());

    assert!(DispatchRoute::Inline.send(&provider, 2_u32).is_ok());
    dispatcher
        .pending
        .borrow_mut()
        .pop()
        .expect("the second subscriber submission was accepted")
        .execute();
    assert_eq!(&*values.borrow(), &[2]);
}

#[test]
fn dropping_a_provider_before_a_queued_publication_executes_prevents_fan_out() {
    let mut dispatcher = FifoDispatcher::new();
    let route = dispatcher.route();
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let handler = EventHandler::new(move |value: u32| output.borrow_mut().push(value));
    let sink = EventSink::from(&handler);
    let (provider, source) = channel::<u32>();
    let input = EventSink::from(&provider);
    source
        .subscribe_cloned(&sink, &route)
        .expect("live provider")
        .detach();
    let trigger_input = input.clone();
    let trigger_route = route.clone();
    let trigger_handler = EventHandler::new(move |value: u32| {
        assert!(trigger_route.send(&trigger_input, value).is_ok());
    });
    let trigger = EventSink::from(&trigger_handler);

    assert!(route.send(&trigger, 7_u32).is_ok());
    assert!(dispatcher.deliver_next());
    drop(provider);
    drain(&mut dispatcher);
    assert!(values.borrow().is_empty());
}

#[test]
fn source_input_submits_subscriptions_in_creation_order() {
    let mut dispatcher = FifoDispatcher::new();
    let route = dispatcher.route();
    let order = Rc::new(RefCell::new(Vec::new()));
    let first_order = Rc::clone(&order);
    let second_order = Rc::clone(&order);
    let first_handler = EventHandler::new(move |_: ()| {
        first_order.borrow_mut().push(1);
    });
    let first = EventSink::from(&first_handler);
    let second_handler = EventHandler::new(move |_: ()| {
        second_order.borrow_mut().push(2);
    });
    let second = EventSink::from(&second_handler);
    let (provider, source) = channel::<()>();
    source
        .subscribe_cloned(&first, &route)
        .expect("live provider")
        .detach();
    source
        .subscribe_cloned(&second, &route)
        .expect("live provider")
        .detach();

    assert!(route.send(&provider, ()).is_ok());
    drain(&mut dispatcher);
    assert_eq!(&*order.borrow(), &[1, 2]);
}

#[test]
fn unsubscribing_does_not_cancel_an_already_queued_subscription_dispatch() {
    let mut dispatcher = FifoDispatcher::new();
    let route = dispatcher.route();
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let handler = EventHandler::new(move |value: u32| output.borrow_mut().push(value));
    let sink = EventSink::from(&handler);
    let (provider, source) = channel::<u32>();
    let subscription = source
        .subscribe_cloned(&sink, &route)
        .expect("live provider");

    assert!(route.send(&provider, 1_u32).is_ok());
    assert!(dispatcher.deliver_next());
    subscription.unsubscribe();
    drain(&mut dispatcher);
    assert_eq!(&*values.borrow(), &[1]);
}

#[test]
fn source_inputs_and_subscribers_keep_their_own_dispatch_routes() {
    let mut first_dispatcher = FifoDispatcher::new();
    let first_route = first_dispatcher.route();
    let mut second_dispatcher = FifoDispatcher::new();
    let second_route = second_dispatcher.route();
    let (provider, source) = channel::<u32>();
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let handler = EventHandler::new(move |value: u32| {
        output.borrow_mut().push(value);
    });
    let sink = EventSink::from(&handler);
    source
        .subscribe_cloned(&sink, &second_route)
        .expect("live provider")
        .detach();

    assert!(first_route.send(&provider, 7_u32).is_ok());
    drain(&mut second_dispatcher);
    assert!(values.borrow().is_empty());
    drain(&mut first_dispatcher);
    assert!(values.borrow().is_empty());
    drain(&mut second_dispatcher);
    assert_eq!(&*values.borrow(), &[7]);
}

#[test]
fn subscribe_converts_from_a_borrowed_event() {
    struct Publication(String);

    #[derive(Debug, PartialEq)]
    struct SubscriptionEvent(usize);

    impl From<&Publication> for SubscriptionEvent {
        fn from(value: &Publication) -> Self {
            Self(value.0.len())
        }
    }

    let mut dispatcher = FifoDispatcher::new();
    let route = dispatcher.route();
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let handler = EventHandler::new(move |value: SubscriptionEvent| {
        output.borrow_mut().push(value);
    });
    let sink = EventSink::from(&handler);
    let (provider, source) = channel::<Publication>();
    source
        .subscribe(&sink, &route)
        .expect("live provider")
        .detach();

    assert!(
        route
            .send(&provider, Publication(String::from("event")))
            .is_ok()
    );
    drain(&mut dispatcher);

    assert_eq!(&*values.borrow(), &[SubscriptionEvent(5)]);
}

#[test]
fn dropping_a_provider_after_publication_executes_keeps_queued_subscription_dispatch() {
    let mut dispatcher = FifoDispatcher::new();
    let route = dispatcher.route();
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let handler = EventHandler::new(move |value: u32| {
        output.borrow_mut().push(value);
    });
    let sink = EventSink::from(&handler);
    let (provider, source) = channel::<u32>();
    source
        .subscribe_cloned(&sink, &route)
        .expect("live provider")
        .detach();

    assert!(route.send(&provider, 3_u32).is_ok());
    assert!(dispatcher.deliver_next());
    drop(provider);
    drain(&mut dispatcher);

    assert_eq!(&*values.borrow(), &[3]);
}
