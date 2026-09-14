use std::{
    cell::{Cell, RefCell},
    fmt,
    rc::Rc,
};

mod queue;

pub use queue::{DispatchQueue, QueueDispatcher};

/// One executable event operation.
///
/// A Dispatch retains its concrete lifetime and auto traits until a
/// Dispatcher accepts it. This lets each Dispatcher state the operations it
/// can retain or move instead of imposing those constraints on every event.
pub trait Dispatch: Sized {
    /// Runs this operation exactly once.
    fn run(self) -> DispatchOutcome;

    /// Attaches local cancellation and terminal-state observation to this Dispatch.
    fn tracked(self) -> TrackedDispatch<Self> {
        TrackedDispatch::new(self)
    }
}

/// The result of running one accepted event Dispatch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DispatchOutcome {
    /// The Dispatch completed normally, including intentional early completion.
    Completed,
    /// The event sink used by this Dispatch is permanently closed.
    ///
    /// This outcome promises that later Dispatches to the same sink cannot
    /// succeed. Temporary refusal must not be reported as `SinkClosed`.
    SinkClosed,
    /// A tracked Dispatch was cancelled before execution began.
    Cancelled,
}

/// Accepts one family of Dispatches for execution or scheduling.
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

/// How far an accepted Dispatch submission progressed before returning.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubmissionOutcome {
    /// The Dispatch ran synchronously with the reported result.
    Executed(DispatchOutcome),
    /// A Dispatcher accepted the Dispatch for later execution.
    Deferred,
}

/// A type-erased Dispatch for heterogeneous storage.
///
/// Erasure preserves the lifetime of the concrete Dispatch. It intentionally
/// does not promise to preserve auto traits such as [`Send`]; storage that
/// needs those properties should use an appropriately constrained erasure.
#[must_use = "a DynDispatch must be run, submitted, or intentionally discarded"]
pub struct DynDispatch<'event> {
    operation: Box<dyn FnOnce() -> DispatchOutcome + 'event>,
}

impl<'event> DynDispatch<'event> {
    /// Erases the concrete type of `dispatch` while preserving its lifetime.
    ///
    /// This is useful at heterogeneous storage boundaries. Code that can keep
    /// a concrete Dispatch type should retain it instead.
    pub fn new<D>(dispatch: D) -> Self
    where
        D: Dispatch + 'event,
    {
        Self {
            operation: Box::new(move || dispatch.run()),
        }
    }
}

impl Dispatch for DynDispatch<'_> {
    fn run(self) -> DispatchOutcome {
        (self.operation)()
    }
}

impl fmt::Debug for DynDispatch<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DynDispatch")
            .finish_non_exhaustive()
    }
}

/// A Dispatch with locally observable cancellation and execution state.
///
/// Tracking uses local shared state and therefore does not make a Dispatch
/// suitable for crossing threads. The wrapped Dispatch otherwise retains its
/// concrete type and lifetime.
#[must_use = "a TrackedDispatch must be run, submitted, or intentionally discarded"]
pub struct TrackedDispatch<D: Dispatch> {
    dispatch: D,
    control: Rc<DispatchControl>,
}

impl<D: Dispatch> TrackedDispatch<D> {
    fn new(dispatch: D) -> Self {
        Self {
            dispatch,
            control: Rc::new(DispatchControl::new()),
        }
    }

    /// Returns a cloneable capability for observing and cancelling this Dispatch.
    pub fn handle(&self) -> DispatchHandle {
        DispatchHandle {
            control: Rc::clone(&self.control),
        }
    }
}

impl<D: Dispatch> Dispatch for TrackedDispatch<D> {
    fn run(self) -> DispatchOutcome {
        let Self { dispatch, control } = self;
        if !control.start() {
            return DispatchOutcome::Cancelled;
        }
        let outcome = dispatch.run();
        control.complete(outcome);
        outcome
    }
}

impl<D: Dispatch> fmt::Debug for TrackedDispatch<D> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TrackedDispatch")
            .finish_non_exhaustive()
    }
}

/// Observes a tracked Dispatch and allows pre-execution cancellation.
///
/// Dropping either the Handle or an unstarted tracked Dispatch does not record
/// cancellation. Cancellation is an explicit operation on the Handle.
#[derive(Clone)]
#[must_use = "dropping a DispatchHandle does not cancel its Dispatch"]
pub struct DispatchHandle {
    control: Rc<DispatchControl>,
}

impl DispatchHandle {
    /// Returns the current execution state.
    pub fn status(&self) -> DispatchStatus {
        self.control.state.get()
    }

    /// Prevents execution if the Dispatch has not started.
    pub fn cancel(&self) -> bool {
        self.control.cancel()
    }

    /// Registers a callback for this Dispatch's terminal outcome.
    pub fn on_finish(&self, notify: impl FnOnce(DispatchOutcome) + 'static) {
        self.control.on_finish(notify);
    }
}

impl fmt::Debug for DispatchHandle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DispatchHandle")
            .finish_non_exhaustive()
    }
}

/// The observable execution state of an accepted tracked Dispatch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DispatchStatus {
    /// Execution has not started and no terminal outcome has been recorded.
    Unstarted,
    /// Execution started but has not produced a terminal outcome.
    Executing,
    /// Execution ended with the reported result.
    Finished(DispatchOutcome),
}

struct DispatchControl {
    state: Cell<DispatchStatus>,
    #[allow(clippy::type_complexity)]
    finish_callbacks: RefCell<Vec<Box<dyn FnOnce(DispatchOutcome)>>>,
}

impl DispatchControl {
    fn new() -> Self {
        Self {
            state: Cell::new(DispatchStatus::Unstarted),
            finish_callbacks: RefCell::new(Vec::new()),
        }
    }

    fn start(&self) -> bool {
        match self.state.get() {
            DispatchStatus::Unstarted => {
                self.state.set(DispatchStatus::Executing);
                true
            }
            DispatchStatus::Finished(DispatchOutcome::Cancelled) => false,
            DispatchStatus::Executing | DispatchStatus::Finished(_) => {
                unreachable!("a Dispatch can only start once")
            }
        }
    }

    fn complete(&self, outcome: DispatchOutcome) {
        debug_assert_eq!(self.state.get(), DispatchStatus::Executing);
        self.finish(outcome);
    }

    fn cancel(&self) -> bool {
        if self.state.get() != DispatchStatus::Unstarted {
            return false;
        }
        self.finish(DispatchOutcome::Cancelled);
        true
    }

    fn on_finish(&self, notify: impl FnOnce(DispatchOutcome) + 'static) {
        if let DispatchStatus::Finished(outcome) = self.state.get() {
            notify(outcome);
        } else {
            self.finish_callbacks.borrow_mut().push(Box::new(notify));
        }
    }

    fn finish(&self, outcome: DispatchOutcome) {
        self.state.set(DispatchStatus::Finished(outcome));
        let callbacks = std::mem::take(&mut *self.finish_callbacks.borrow_mut());
        for notify in callbacks {
            notify(outcome);
        }
    }
}
