use std::rc::Rc;

use theoria_event::{
    Dispatch, Dispatcher, DynDispatch, EventSink, EventSource, SubmissionOutcome, Subscription,
};

/// The type-erasure boundary for one Component tree's default delivery.
#[derive(Clone)]
pub(crate) struct DefaultDispatcher {
    dispatcher: Rc<dyn Dispatcher<DynDispatch<'static>>>,
}

impl DefaultDispatcher {
    pub(crate) fn new(dispatcher: impl Dispatcher<DynDispatch<'static>> + 'static) -> Self {
        Self {
            dispatcher: Rc::new(dispatcher),
        }
    }

    pub(crate) fn submit<D>(&self, dispatch: D) -> Result<SubmissionOutcome, DynDispatch<'static>>
    where
        D: Dispatch + 'static,
    {
        self.dispatcher.submit(DynDispatch::new(dispatch))
    }

    pub(crate) fn connect<S, K>(&self, source: S, sink: K) -> Option<Subscription>
    where
        S: EventSource,
        K: 'static,
        for<'sink, 'event> &'sink K: EventSink<S::Event<'event>>,
        for<'sink, 'event> <&'sink K as EventSink<S::Event<'event>>>::Dispatch: 'static,
    {
        let dispatcher = self.clone();
        let unregister = source.register(move |event| {
            let dispatch = EventSink::send(&sink, event);
            let _ = dispatcher.submit(DynDispatch::new(dispatch));
        })?;
        Some(Subscription::new(unregister))
    }
}

impl Dispatcher<DynDispatch<'static>> for DefaultDispatcher {
    fn submit(
        &self,
        dispatch: DynDispatch<'static>,
    ) -> Result<SubmissionOutcome, DynDispatch<'static>> {
        self.dispatcher.submit(dispatch)
    }
}
