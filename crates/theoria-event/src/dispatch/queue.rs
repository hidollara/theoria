use std::{
    cell::RefCell,
    collections::VecDeque,
    fmt,
    rc::{Rc, Weak},
};

use super::{Dispatch, DispatchOutcome, Dispatcher, DynDispatch, SubmissionOutcome};

type PendingDispatches = RefCell<VecDeque<DynDispatch<'static>>>;

/// Uniquely owns a local FIFO queue of deferred Dispatches.
pub struct DispatchQueue {
    pending: Rc<PendingDispatches>,
}

impl DispatchQueue {
    /// Creates an empty Dispatch queue.
    pub fn new() -> Self {
        Self {
            pending: Rc::new(RefCell::new(VecDeque::new())),
        }
    }

    /// Returns a cloneable, non-owning Dispatcher that submits to this queue.
    pub fn dispatcher(&self) -> QueueDispatcher {
        QueueDispatcher {
            pending: Rc::downgrade(&self.pending),
        }
    }

    /// Returns whether no Dispatch is waiting to execute.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.pending.borrow().is_empty()
    }

    /// Executes and removes the next queued Dispatch.
    ///
    /// Returns `None` when the queue is empty. Work submitted during execution
    /// joins the tail and remains deferred until a later call, so the queue
    /// never executes recursively. Dropping the queue discards its remaining
    /// Dispatches without recording cancellation.
    #[must_use]
    pub fn run_next(&mut self) -> Option<DispatchOutcome> {
        let dispatch = self.pending.borrow_mut().pop_front()?;
        Some(dispatch.run())
    }
}

impl Default for DispatchQueue {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for DispatchQueue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DispatchQueue")
            .finish_non_exhaustive()
    }
}

/// A cloneable, non-owning Dispatcher for one [`DispatchQueue`].
#[derive(Clone)]
pub struct QueueDispatcher {
    pending: Weak<PendingDispatches>,
}

impl<D> Dispatcher<D> for QueueDispatcher
where
    D: Dispatch + 'static,
{
    fn submit(&self, dispatch: D) -> Result<SubmissionOutcome, D> {
        let Some(pending) = self.pending.upgrade() else {
            return Err(dispatch);
        };
        pending.borrow_mut().push_back(DynDispatch::new(dispatch));
        Ok(SubmissionOutcome::Deferred)
    }
}

impl fmt::Debug for QueueDispatcher {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("QueueDispatcher")
            .finish_non_exhaustive()
    }
}
