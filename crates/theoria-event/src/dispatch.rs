use std::{
    any::Any,
    cell::{Cell, RefCell},
    fmt,
    rc::Rc,
};

use crate::sink::EventSink;

type CompletionFn = Box<dyn FnOnce(DispatchOutcome)>;

/// The result of executing one accepted event Dispatch.
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
    ///
    /// A Sink panic is not caught, so an unwound Dispatch remains in this
    /// state.
    Executing,
    /// Execution ended with the reported result.
    Finished(DispatchOutcome),
}

/// Accepts type-erased event Dispatches for execution or scheduling.
///
/// A successful return reports whether the accepted Dispatch completed during
/// the call or was retained for later execution.
pub trait EventDispatcher: 'static {
    /// Submits one Dispatch for immediate or deferred execution.
    ///
    /// Rejection returns the original Dispatch without executing or retaining
    /// it, so another Dispatcher may accept it.
    fn submit(&self, dispatch: EventDispatch) -> Result<SubmissionOutcome, EventDispatch>;
}

impl<T> EventDispatcher for Rc<T>
where
    T: EventDispatcher + ?Sized,
{
    fn submit(&self, dispatch: EventDispatch) -> Result<SubmissionOutcome, EventDispatch> {
        self.as_ref().submit(dispatch)
    }
}

/// A cloneable capability for submitting owned event Dispatches.
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

    /// Creates a Dispatcher that executes accepted Dispatches immediately.
    ///
    /// This preserves Dispatcher semantics, including owned type-erased events
    /// and tracked Receipts. Use [`EventSink::send`] when direct delivery
    /// without the `E: 'static` dispatch boundary is intended.
    pub fn immediate() -> Self {
        Self::new(ImmediateDispatcher)
    }

    /// Submits one owned event to this Dispatcher.
    ///
    /// A rejecting Dispatcher returns ownership of the original event. A
    /// successful submission reports whether its Dispatch completed on this
    /// call stack or was deferred.
    pub fn send<E>(&self, sink: impl Into<EventSink<E>>, event: E) -> Result<SubmissionOutcome, E>
    where
        E: 'static,
    {
        self.endpoint
            .submit(EventDispatch::new(sink.into(), event))
            .map_err(EventDispatch::into_event)
    }

    /// Submits one tracked event to this Dispatcher.
    ///
    /// A rejecting Dispatcher returns ownership of the original event and no
    /// Receipt. Otherwise the returned Receipt observes execution, Sink
    /// unavailability, cancellation, and completion callbacks.
    pub fn send_tracked<E>(
        &self,
        sink: impl Into<EventSink<E>>,
        event: E,
    ) -> Result<DispatchReceipt, E>
    where
        E: 'static,
    {
        let sink = sink.into();
        let control = Rc::new(DispatchControl::new());
        let receipt = DispatchReceipt {
            control: Rc::clone(&control),
        };

        self.endpoint
            .submit(EventDispatch::tracked(sink, event, control))
            .map(|_| receipt)
            .map_err(EventDispatch::into_event)
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
    fn submit(&self, dispatch: EventDispatch) -> Result<SubmissionOutcome, EventDispatch> {
        Ok(SubmissionOutcome::Completed(dispatch.execute()))
    }
}

/// Observes an accepted tracked Dispatch and allows pre-execution cancellation.
///
/// Acceptance does not imply that event handling has completed. Dropping the
/// Receipt does not cancel its Dispatch. Dropping an accepted tracked Dispatch
/// before execution cancels it. Sink panics propagate without producing a
/// terminal outcome or invoking completion callbacks.
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
    ///
    /// Returns `true` when this call cancelled pending work. Returns `false`
    /// when it was already cancelled or had started executing.
    pub fn cancel(&self) -> bool {
        self.control.cancel()
    }

    /// Registers a callback for this Dispatch's terminal outcome.
    ///
    /// If the Dispatch has already finished, the callback runs immediately on
    /// the current call stack. Otherwise the Dispatch retains the callback and
    /// invokes it once on the call stack that finishes execution or
    /// cancellation. Callbacks registered before completion run in
    /// registration order. Dropping the Receipt after registration does not
    /// remove them. A callback panic propagates normally and prevents later
    /// callbacks from running.
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

/// An opaque, type-erased invocation of one event Sink.
///
/// A Dispatcher may execute, retain, reorder, or reject this value. Its event
/// type and Sink remain private. A rejecting Dispatcher must return the
/// original unexecuted value.
#[must_use = "an EventDispatch must be executed, retained, returned, or intentionally discarded"]
pub struct EventDispatch {
    erased: Box<dyn ErasedDispatch>,
}

impl EventDispatch {
    fn new<E>(sink: EventSink<E>, event: E) -> Self
    where
        E: 'static,
    {
        Self {
            erased: Box::new(TypedDispatch {
                sink,
                event,
                control: DispatchControlGuard::untracked(),
            }),
        }
    }

    fn tracked<E>(sink: EventSink<E>, event: E, control: Rc<DispatchControl>) -> Self
    where
        E: 'static,
    {
        Self {
            erased: Box::new(TypedDispatch {
                sink,
                event,
                control: DispatchControlGuard::tracked(control),
            }),
        }
    }

    /// Executes this Sink invocation exactly once and reports its outcome.
    ///
    /// A previously cancelled tracked Dispatch does not invoke its Sink and
    /// reports [`DispatchOutcome::Cancelled`].
    pub fn execute(self) -> DispatchOutcome {
        self.erased.execute()
    }

    pub(crate) fn into_event<E: 'static>(self) -> E {
        *self
            .erased
            .into_event()
            .downcast::<E>()
            .expect("a rejected EventDispatch must retain its original event type")
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

impl fmt::Debug for EventDispatch {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EventDispatch")
            .finish_non_exhaustive()
    }
}

trait ErasedDispatch {
    fn execute(self: Box<Self>) -> DispatchOutcome;

    fn into_event(self: Box<Self>) -> Box<dyn Any>;
}

struct TypedDispatch<E: 'static> {
    sink: EventSink<E>,
    event: E,
    control: DispatchControlGuard,
}

impl<E: 'static> ErasedDispatch for TypedDispatch<E> {
    fn execute(self: Box<Self>) -> DispatchOutcome {
        let TypedDispatch {
            sink,
            event,
            control,
        } = *self;
        let Some(control) = control.disarm() else {
            return if sink.send(event).is_ok() {
                DispatchOutcome::Delivered
            } else {
                DispatchOutcome::SinkUnavailable
            };
        };
        if !control.start() {
            return DispatchOutcome::Cancelled;
        }
        let outcome = if sink.send(event).is_ok() {
            DispatchOutcome::Delivered
        } else {
            DispatchOutcome::SinkUnavailable
        };
        control.complete(outcome);
        outcome
    }

    fn into_event(self: Box<Self>) -> Box<dyn Any> {
        let TypedDispatch { event, .. } = *self;
        Box::new(event)
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
