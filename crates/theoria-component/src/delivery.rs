use std::rc::Rc;

use theoria_event::{Dispatch, DynDispatch, EventDispatcher, SubmissionOutcome};

/// The type-erasure boundary for one Component tree's default delivery.
#[derive(Clone)]
pub(crate) struct DefaultDispatcher {
    dispatcher: Rc<dyn EventDispatcher<DynDispatch<'static>>>,
}

impl DefaultDispatcher {
    pub(crate) fn new(dispatcher: impl EventDispatcher<DynDispatch<'static>> + 'static) -> Self {
        Self {
            dispatcher: Rc::new(dispatcher),
        }
    }
}

impl<D> EventDispatcher<D> for DefaultDispatcher
where
    D: Dispatch + 'static,
{
    fn submit(&self, dispatch: D) -> Result<SubmissionOutcome, D> {
        let erased = DynDispatch::recoverable(dispatch);
        match self.dispatcher.submit(erased) {
            Ok(outcome) => Ok(outcome),
            Err(erased) => Err(erased
                .downcast()
                .expect("default delivery must return the Dispatch it rejected")),
        }
    }
}
