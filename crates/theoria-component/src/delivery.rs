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
        for<'event> K: EventSink<S::Event<'event>>,
        for<'event> <K as EventSink<S::Event<'event>>>::Dispatch: 'static,
    {
        source.connect(ErasingSink(sink)).via(self.clone())
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

struct ErasingSink<S>(S);

impl<E, S> EventSink<E> for ErasingSink<S>
where
    S: EventSink<E>,
    S::Dispatch: 'static,
{
    type Dispatch = DynDispatch<'static>;

    fn send(&self, event: E) -> Self::Dispatch {
        DynDispatch::new(self.0.send(event))
    }
}
