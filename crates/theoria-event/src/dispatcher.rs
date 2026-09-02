use std::{
    cell::RefCell,
    collections::VecDeque,
    fmt,
    rc::{Rc, Weak},
};

use crate::dispatch::{DispatchRoute, EventDispatch, EventDispatcher, SubmissionOutcome};

struct EventQueue {
    pending: RefCell<VecDeque<EventDispatch>>,
}

impl EventQueue {
    fn new() -> Self {
        Self {
            pending: RefCell::new(VecDeque::new()),
        }
    }

    fn enqueue(&self, dispatch: EventDispatch) {
        self.pending.borrow_mut().push_back(dispatch);
    }

    fn deliver_next(&self) -> bool {
        let Some(dispatch) = self.pending.borrow_mut().pop_front() else {
            return false;
        };
        let _ = dispatch.execute();
        true
    }
}

/// Uniquely owns a serialized FIFO Dispatch queue.
pub struct FifoDispatcher {
    queue: Rc<EventQueue>,
    route: DispatchRoute,
}

impl FifoDispatcher {
    /// Creates an idle FIFO Dispatcher.
    pub fn new() -> Self {
        let queue = Rc::new(EventQueue::new());
        Self {
            route: DispatchRoute::Dispatcher(Rc::new(QueuedDispatcher {
                queue: Rc::downgrade(&queue),
            })),
            queue,
        }
    }

    /// Returns a route through this Dispatcher.
    pub fn route(&self) -> DispatchRoute {
        self.route.clone()
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

struct QueuedDispatcher {
    queue: Weak<EventQueue>,
}

impl EventDispatcher for QueuedDispatcher {
    fn submit(&self, dispatch: EventDispatch) -> Result<SubmissionOutcome, EventDispatch> {
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
