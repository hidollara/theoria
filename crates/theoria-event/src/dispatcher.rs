use std::{
    cell::RefCell,
    collections::VecDeque,
    fmt,
    rc::{Rc, Weak},
};

use crate::dispatch::{Dispatch, Dispatcher, EventDispatch, EventDispatcher, SubmissionOutcome};

struct EventQueue {
    pending: RefCell<VecDeque<EventDispatch<'static>>>,
}

impl EventQueue {
    fn new() -> Self {
        Self {
            pending: RefCell::new(VecDeque::new()),
        }
    }

    fn enqueue(&self, dispatch: EventDispatch<'static>) {
        self.pending.borrow_mut().push_back(dispatch);
    }

    fn deliver_next(&self) -> bool {
        let Some(dispatch) = self.pending.borrow_mut().pop_front() else {
            return false;
        };
        let _ = dispatch.run();
        true
    }

    fn has_pending(&self) -> bool {
        !self.pending.borrow().is_empty()
    }
}

/// Uniquely owns a serialized FIFO Dispatch queue.
pub struct FifoDispatcher {
    queue: Rc<EventQueue>,
    dispatcher: Dispatcher,
}

impl FifoDispatcher {
    /// Creates an idle FIFO Dispatcher.
    pub fn new() -> Self {
        let queue = Rc::new(EventQueue::new());
        Self {
            dispatcher: Dispatcher::new(QueuedDispatcher {
                queue: Rc::downgrade(&queue),
            }),
            queue,
        }
    }

    /// Returns whether at least one Dispatch is waiting to execute.
    #[must_use]
    pub fn has_pending(&self) -> bool {
        self.queue.has_pending()
    }

    /// Executes the next queued Dispatch.
    ///
    /// Returns `true` when exactly one queued entry was consumed, regardless
    /// of its terminal outcome. Returns `false` when the queue was empty. Work
    /// submitted during execution remains deferred until a later call, so this
    /// Dispatcher never executes recursively. Dropping the Dispatcher cancels
    /// every pending tracked Dispatch.
    #[must_use]
    pub fn deliver_next(&mut self) -> bool {
        self.queue.deliver_next()
    }
}

impl AsRef<Dispatcher> for FifoDispatcher {
    fn as_ref(&self) -> &Dispatcher {
        &self.dispatcher
    }
}

impl<D> EventDispatcher<D> for FifoDispatcher
where
    D: Dispatch + 'static,
{
    fn submit(&self, dispatch: D) -> Result<SubmissionOutcome, D> {
        self.queue.enqueue(EventDispatch::erase(dispatch));
        Ok(SubmissionOutcome::Deferred)
    }
}

struct QueuedDispatcher {
    queue: Weak<EventQueue>,
}

impl EventDispatcher<EventDispatch<'static>> for QueuedDispatcher {
    fn submit(
        &self,
        dispatch: EventDispatch<'static>,
    ) -> Result<SubmissionOutcome, EventDispatch<'static>> {
        let Some(queue) = self.queue.upgrade() else {
            return Err(dispatch);
        };
        queue.enqueue(dispatch);
        Ok(SubmissionOutcome::Deferred)
    }
}

impl Default for FifoDispatcher {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for FifoDispatcher {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FifoDispatcher")
            .finish_non_exhaustive()
    }
}
