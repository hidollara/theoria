use std::{
    any::Any,
    cell::{Cell, RefCell},
    fmt,
    rc::Rc,
};

type CompletionFn = Box<dyn FnOnce(DispatchOutcome)>;

/// One executable event operation.
///
/// A Dispatch retains its concrete lifetime and auto traits until a
/// Dispatcher accepts it. This lets each Dispatcher state the operations it
/// can retain or move instead of imposing those constraints on every event.
pub trait Dispatch: Sized {
    /// Runs this operation exactly once.
    fn run(self) -> DispatchOutcome;

    /// Attaches local cancellation and completion observation to this Dispatch.
    fn tracked(self) -> (TrackedDispatch<Self>, DispatchReceipt) {
        TrackedDispatch::new(self)
    }
}

/// The result of running one accepted event Dispatch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DispatchOutcome {
    /// The Dispatch reached a live event sink and completed there.
    Delivered,
    /// The event sink's endpoint had been dropped before execution.
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

/// Accepts one family of event Dispatches for execution or scheduling.
///
/// Implementations express their requirements through `D`: an inline
/// Dispatcher may accept borrowed work, a local queue may require `'static`,
/// and a cross-thread Dispatcher may additionally require [`Send`].
pub trait EventDispatcher<D>
where
    D: Dispatch,
{
    /// Submits one Dispatch for immediate or deferred execution.
    ///
    /// Rejection returns the original Dispatch without running or retaining
    /// it, so another Dispatcher may accept it.
    fn submit(&self, dispatch: D) -> Result<SubmissionOutcome, D>;
}

impl<D, T> EventDispatcher<D> for Rc<T>
where
    D: Dispatch,
    T: EventDispatcher<D> + ?Sized,
{
    fn submit(&self, dispatch: D) -> Result<SubmissionOutcome, D> {
        self.as_ref().submit(dispatch)
    }
}

/// A cloneable capability for submitting self-contained event Dispatches.
#[derive(Clone)]
pub struct Dispatcher {
    endpoint: Rc<dyn EventDispatcher<EventDispatch<'static>>>,
}

impl Dispatcher {
    /// Wraps an event Dispatcher as a cloneable submission capability.
    pub fn new(dispatcher: impl EventDispatcher<EventDispatch<'static>> + 'static) -> Self {
        Self {
            endpoint: Rc::new(dispatcher),
        }
    }

    /// Creates a Dispatcher that runs accepted Dispatches immediately.
    pub fn immediate() -> Self {
        Self::new(ImmediateDispatcher)
    }

    /// Submits one self-contained local Dispatch.
    pub fn submit<D>(&self, dispatch: D) -> Result<SubmissionOutcome, D>
    where
        D: Dispatch + 'static,
    {
        <Self as EventDispatcher<D>>::submit(self, dispatch)
    }
}

impl<D> EventDispatcher<D> for Dispatcher
where
    D: Dispatch + 'static,
{
    fn submit(&self, dispatch: D) -> Result<SubmissionOutcome, D> {
        let erased = EventDispatch::recoverable(dispatch);

        match self.endpoint.submit(erased) {
            Ok(outcome) => Ok(outcome),
            Err(erased) => Err(erased.into_concrete()),
        }
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

/// A Dispatcher that runs any accepted Dispatch before returning.
#[derive(Clone, Copy, Debug, Default)]
pub struct ImmediateDispatcher;

impl<D> EventDispatcher<D> for ImmediateDispatcher
where
    D: Dispatch,
{
    fn submit(&self, dispatch: D) -> Result<SubmissionOutcome, D> {
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

/// One invocation of an event sink.
///
/// The Dispatch owns its event and every reusable operation copied from its
/// sink. Its lifetime is bounded by the shortest-lived value captured by that
/// invocation. Any Dispatch may run immediately; only a `'static` Dispatch may
/// be submitted to a [`Dispatcher`].
#[must_use = "an EventDispatch must be run, submitted, or intentionally discarded"]
pub struct EventDispatch<'event> {
    operation: DispatchOperation<'event>,
}

impl<'event> EventDispatch<'event> {
    pub(crate) fn new(operation: impl FnOnce() -> DispatchOutcome + 'event) -> Self {
        Self {
            operation: DispatchOperation::Scoped(Box::new(operation)),
        }
    }

    /// Runs this event-sink invocation exactly once.
    pub fn run(self) -> DispatchOutcome {
        self.operation.run()
    }

    /// Erases the concrete type of `dispatch` while preserving its lifetime.
    ///
    /// This is useful at heterogeneous storage boundaries. Code that can keep
    /// a concrete Dispatch type should retain it instead.
    pub fn erase<D>(dispatch: D) -> Self
    where
        D: Dispatch + 'event,
    {
        Self::new(move || dispatch.run())
    }

    fn recoverable<D>(dispatch: D) -> EventDispatch<'static>
    where
        D: Dispatch + 'static,
    {
        EventDispatch {
            operation: DispatchOperation::Recoverable(Box::new(ConcreteDispatch(dispatch))),
        }
    }
}

impl EventDispatch<'static> {
    fn into_concrete<D>(self) -> D
    where
        D: Dispatch + 'static,
    {
        let EventDispatch { operation } = self;
        let DispatchOperation::Recoverable(operation) = operation else {
            panic!("a rejecting Dispatcher must return its original Dispatch");
        };
        *operation
            .into_any()
            .downcast::<D>()
            .unwrap_or_else(|_| panic!("a rejecting Dispatcher changed its Dispatch type"))
    }
}

impl Dispatch for EventDispatch<'_> {
    fn run(self) -> DispatchOutcome {
        EventDispatch::run(self)
    }
}

impl fmt::Debug for EventDispatch<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EventDispatch")
            .finish_non_exhaustive()
    }
}

/// A Dispatch with locally observable cancellation and completion state.
///
/// Tracking uses local shared state and therefore does not make a Dispatch
/// suitable for crossing threads. The wrapped Dispatch otherwise retains its
/// concrete type and lifetime.
#[must_use = "a TrackedDispatch must be run, submitted, or intentionally discarded"]
pub struct TrackedDispatch<D: Dispatch> {
    dispatch: D,
    control: DispatchControlGuard,
}

impl<D: Dispatch> TrackedDispatch<D> {
    fn new(dispatch: D) -> (Self, DispatchReceipt) {
        let control = Rc::new(DispatchControl::new());
        (
            Self {
                dispatch,
                control: DispatchControlGuard::new(Rc::clone(&control)),
            },
            DispatchReceipt { control },
        )
    }
}

impl<D: Dispatch> Dispatch for TrackedDispatch<D> {
    fn run(self) -> DispatchOutcome {
        let Self { dispatch, control } = self;
        let control = control.disarm();
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

enum DispatchOperation<'event> {
    Scoped(Box<dyn FnOnce() -> DispatchOutcome + 'event>),
    Recoverable(Box<dyn RecoverableDispatch>),
}

impl DispatchOperation<'_> {
    fn run(self) -> DispatchOutcome {
        match self {
            Self::Scoped(operation) => operation(),
            Self::Recoverable(operation) => operation.run(),
        }
    }
}

trait RecoverableDispatch {
    fn run(self: Box<Self>) -> DispatchOutcome;

    fn into_any(self: Box<Self>) -> Box<dyn Any>;
}

struct ConcreteDispatch<D>(D);

impl<D> RecoverableDispatch for ConcreteDispatch<D>
where
    D: Dispatch + 'static,
{
    fn run(self: Box<Self>) -> DispatchOutcome {
        self.0.run()
    }

    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        Box::new(self.0)
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
    fn new(control: Rc<DispatchControl>) -> Self {
        Self(Some(control))
    }

    fn disarm(mut self) -> Rc<DispatchControl> {
        self.0
            .take()
            .expect("a tracked Dispatch retains its control until execution")
    }
}

impl Drop for DispatchControlGuard {
    fn drop(&mut self) {
        if let Some(control) = &self.0 {
            control.cancel();
        }
    }
}
