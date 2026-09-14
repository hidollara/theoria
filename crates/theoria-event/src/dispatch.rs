use std::{
    cell::{Cell, RefCell},
    fmt,
    rc::Rc,
};

type CompletionFn = Box<dyn FnOnce(DispatchOutcome)>;

/// The result of running one accepted event Dispatch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DispatchOutcome {
    /// The Dispatch reached a live Sink and completed there.
    Delivered,
    /// The Sink's endpoint had been dropped before execution.
    SinkUnavailable,
    /// A tracked Dispatch was cancelled before execution began.
    Cancelled,
}

/// How far an accepted event submission progressed before returning.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubmissionOutcome {
    /// The Dispatch completed synchronously with the reported result.
    Completed(DispatchOutcome),
    /// A Dispatcher accepted the Dispatch for later execution.
    Deferred,
}

/// The observable execution state of an accepted tracked Dispatch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DispatchStatus {
    /// The Dispatch has been accepted but has not started.
    Pending,
    /// Execution started but has not produced a terminal outcome.
    Executing,
    /// Execution ended with the reported result.
    Finished(DispatchOutcome),
}

/// Accepts self-contained event Dispatches for execution or scheduling.
pub trait EventDispatcher: 'static {
    /// Submits one Dispatch for immediate or deferred execution.
    ///
    /// Rejection returns the original Dispatch without running or retaining
    /// it, so another Dispatcher may accept it.
    fn submit(
        &self,
        dispatch: EventDispatch<'static>,
    ) -> Result<SubmissionOutcome, EventDispatch<'static>>;
}

impl<T> EventDispatcher for Rc<T>
where
    T: EventDispatcher + ?Sized,
{
    fn submit(
        &self,
        dispatch: EventDispatch<'static>,
    ) -> Result<SubmissionOutcome, EventDispatch<'static>> {
        self.as_ref().submit(dispatch)
    }
}

/// A cloneable capability for submitting self-contained event Dispatches.
#[derive(Clone)]
pub struct Dispatcher {
    endpoint: Rc<dyn EventDispatcher>,
}

impl Dispatcher {
    /// Wraps an event Dispatcher as a cloneable submission capability.
    pub fn new(dispatcher: impl EventDispatcher) -> Self {
        Self {
            endpoint: Rc::new(dispatcher),
        }
    }

    /// Creates a Dispatcher that runs accepted Dispatches immediately.
    pub fn immediate() -> Self {
        Self::new(ImmediateDispatcher)
    }

    /// Submits one self-contained Dispatch.
    pub fn submit(
        &self,
        dispatch: EventDispatch<'static>,
    ) -> Result<SubmissionOutcome, EventDispatch<'static>> {
        self.endpoint.submit(dispatch)
    }
}

impl AsRef<Dispatcher> for Dispatcher {
    fn as_ref(&self) -> &Dispatcher {
        self
    }
}

impl fmt::Debug for Dispatcher {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("Dispatcher").finish_non_exhaustive()
    }
}

struct ImmediateDispatcher;

impl EventDispatcher for ImmediateDispatcher {
    fn submit(
        &self,
        dispatch: EventDispatch<'static>,
    ) -> Result<SubmissionOutcome, EventDispatch<'static>> {
        Ok(SubmissionOutcome::Completed(dispatch.run()))
    }
}

/// Observes a tracked Dispatch and allows pre-execution cancellation.
///
/// Dropping the Receipt does not cancel its Dispatch. Dropping a tracked
/// Dispatch before execution does cancel it.
#[derive(Clone)]
#[must_use = "dropping a DispatchReceipt does not cancel its Dispatch"]
pub struct DispatchReceipt {
    control: Rc<DispatchControl>,
}

impl DispatchReceipt {
    /// Returns the current execution state.
    pub fn status(&self) -> DispatchStatus {
        self.control.state.get().into()
    }

    /// Prevents execution if the Dispatch has not started.
    pub fn cancel(&self) -> bool {
        self.control.cancel()
    }

    /// Registers a callback for this Dispatch's terminal outcome.
    pub fn on_complete(&self, notify: impl FnOnce(DispatchOutcome) + 'static) {
        self.control.on_complete(notify);
    }
}

impl fmt::Debug for DispatchReceipt {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DispatchReceipt")
            .finish_non_exhaustive()
    }
}

/// One invocation of an event Sink.
///
/// The Dispatch owns its event and every reusable operation copied from its
/// Sink. Its lifetime is bounded by the shortest-lived value captured by that
/// invocation. Any Dispatch may run immediately; only a `'static` Dispatch may
/// be submitted to a [`Dispatcher`].
#[must_use = "an EventDispatch must be run, submitted, or intentionally discarded"]
pub struct EventDispatch<'event> {
    operation: Box<dyn FnOnce() -> DispatchOutcome + 'event>,
    control: DispatchControlGuard,
}

impl<'event> EventDispatch<'event> {
    pub(crate) fn new(operation: impl FnOnce() -> DispatchOutcome + 'event) -> Self {
        Self {
            operation: Box::new(operation),
            control: DispatchControlGuard::untracked(),
        }
    }

    /// Attaches observable cancellation and completion state.
    pub fn tracked(mut self) -> (Self, DispatchReceipt) {
        debug_assert!(self.control.0.is_none());
        let control = Rc::new(DispatchControl::new());
        self.control = DispatchControlGuard::tracked(Rc::clone(&control));
        (self, DispatchReceipt { control })
    }

    /// Runs this Sink invocation exactly once.
    pub fn run(self) -> DispatchOutcome {
        let EventDispatch { operation, control } = self;
        let Some(control) = control.disarm() else {
            return operation();
        };
        if !control.start() {
            return DispatchOutcome::Cancelled;
        }
        let outcome = operation();
        control.complete(outcome);
        outcome
    }
}

impl fmt::Debug for EventDispatch<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EventDispatch")
            .finish_non_exhaustive()
    }
}

struct DispatchControl {
    state: Cell<DispatchState>,
    completions: RefCell<Vec<CompletionFn>>,
}

impl DispatchControl {
    fn new() -> Self {
        Self {
            state: Cell::new(DispatchState::Pending),
            completions: RefCell::new(Vec::new()),
        }
    }

    fn start(&self) -> bool {
        if self.state.get() != DispatchState::Pending {
            return false;
        }
        self.state.set(DispatchState::Executing);
        true
    }

    fn complete(&self, outcome: DispatchOutcome) {
        debug_assert_eq!(self.state.get(), DispatchState::Executing);
        self.finish(outcome);
    }

    fn cancel(&self) -> bool {
        if self.state.get() != DispatchState::Pending {
            return false;
        }
        self.finish(DispatchOutcome::Cancelled);
        true
    }

    fn on_complete(&self, notify: impl FnOnce(DispatchOutcome) + 'static) {
        if let DispatchState::Finished(outcome) = self.state.get() {
            notify(outcome);
        } else {
            self.completions.borrow_mut().push(Box::new(notify));
        }
    }

    fn finish(&self, outcome: DispatchOutcome) {
        self.state.set(DispatchState::Finished(outcome));
        let completions = std::mem::take(&mut *self.completions.borrow_mut());
        for notify in completions {
            notify(outcome);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DispatchState {
    Pending,
    Executing,
    Finished(DispatchOutcome),
}

impl From<DispatchState> for DispatchStatus {
    fn from(state: DispatchState) -> Self {
        match state {
            DispatchState::Pending => Self::Pending,
            DispatchState::Executing => Self::Executing,
            DispatchState::Finished(outcome) => Self::Finished(outcome),
        }
    }
}

struct DispatchControlGuard(Option<Rc<DispatchControl>>);

impl DispatchControlGuard {
    fn untracked() -> Self {
        Self(None)
    }

    fn tracked(control: Rc<DispatchControl>) -> Self {
        Self(Some(control))
    }

    fn disarm(mut self) -> Option<Rc<DispatchControl>> {
        self.0.take()
    }
}

impl Drop for DispatchControlGuard {
    fn drop(&mut self) {
        if let Some(control) = &self.0 {
            control.cancel();
        }
    }
}
