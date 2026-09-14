use std::{
    cell::RefCell,
    collections::VecDeque,
    fmt,
    rc::{Rc, Weak},
};

use crate::dispatch::{Dispatch, DispatchOutcome, DynDispatch};

/// How far an accepted event submission progressed before returning.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubmissionOutcome {
    /// The Dispatch ran synchronously with the reported result.
    Executed(DispatchOutcome),
    /// A Dispatcher accepted the Dispatch for later execution.
    Deferred,
}

/// Accepts one family of event Dispatches for execution or scheduling.
///
/// Implementations express their requirements through `D`: an inline
/// Dispatcher may accept borrowed work, a local queue may require `'static`,
/// and a cross-thread Dispatcher may additionally require [`Send`].
pub trait Dispatcher<D>
where
    D: Dispatch,
{
    /// Submits one Dispatch for immediate or deferred execution.
    ///
    /// Rejection returns the original Dispatch without running or retaining
    /// it, so another Dispatcher may accept it.
    fn submit(&self, dispatch: D) -> Result<SubmissionOutcome, D>;
}

struct EventQueue {
    pending: RefCell<VecDeque<DynDispatch<'static>>>,
}

impl EventQueue {
    fn new() -> Self {
        Self {
            pending: RefCell::new(VecDeque::new()),
        }
    }

    fn enqueue(&self, dispatch: DynDispatch<'static>) {
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
}

impl FifoDispatcher {
    /// Creates an idle FIFO Dispatcher.
    pub fn new() -> Self {
        Self {
            queue: Rc::new(EventQueue::new()),
        }
    }

    /// Returns a cloneable, non-owning capability for submitting Dispatches to
    /// this FIFO.
    pub fn sender(&self) -> FifoSender {
        FifoSender {
            queue: Rc::downgrade(&self.queue),
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
    /// Dispatcher never executes recursively. Dropping the Dispatcher discards
    /// its queued Dispatches without recording cancellation.
    #[must_use]
    pub fn deliver_next(&mut self) -> bool {
        self.queue.deliver_next()
    }
}

impl<D> Dispatcher<D> for FifoDispatcher
where
    D: Dispatch + 'static,
{
    fn submit(&self, dispatch: D) -> Result<SubmissionOutcome, D> {
        self.queue.enqueue(DynDispatch::new(dispatch));
        Ok(SubmissionOutcome::Deferred)
    }
}

/// A cloneable, non-owning capability for submitting Dispatches to a
/// [`FifoDispatcher`].
#[derive(Clone)]
pub struct FifoSender {
    queue: Weak<EventQueue>,
}

impl<D> Dispatcher<D> for FifoSender
where
    D: Dispatch + 'static,
{
    fn submit(&self, dispatch: D) -> Result<SubmissionOutcome, D> {
        let Some(queue) = self.queue.upgrade() else {
            return Err(dispatch);
        };
        queue.enqueue(DynDispatch::new(dispatch));
        Ok(SubmissionOutcome::Deferred)
    }
}

impl fmt::Debug for FifoSender {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("FifoSender").finish_non_exhaustive()
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
