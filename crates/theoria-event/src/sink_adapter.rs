use std::{fmt, marker::PhantomData};

use crate::{
    dispatch::{Dispatch, DispatchOutcome},
    sink::EventSink,
};

/// Adapts an event sink by converting its input when the resulting Dispatch runs.
///
/// Sending `T` does not run `map`. The returned [`ContramapDispatch`] retains
/// `T`, the conversion, and the underlying event sink until execution, so their
/// lifetimes and auto traits remain visible to a prospective Dispatcher.
pub struct ContramapSink<S, M, E> {
    sink: S,
    map: M,
    event: PhantomData<fn(E)>,
}

impl<S, M, E> ContramapSink<S, M, E> {
    /// Wraps `sink` with a conversion performed during Dispatch execution.
    pub const fn new(sink: S, map: M) -> Self {
        Self {
            sink,
            map,
            event: PhantomData,
        }
    }
}

impl<T, E, S, M> EventSink<T> for ContramapSink<S, M, E>
where
    S: EventSink<E> + Clone,
    M: Fn(T) -> E + Clone,
{
    type Dispatch = ContramapDispatch<S, M, T, E>;

    fn send(&self, event: T) -> Self::Dispatch {
        ContramapDispatch {
            sink: self.sink.clone(),
            map: self.map.clone(),
            input: event,
            event: PhantomData,
        }
    }
}

impl<S, M, E> Clone for ContramapSink<S, M, E>
where
    S: Clone,
    M: Clone,
{
    fn clone(&self) -> Self {
        Self::new(self.sink.clone(), self.map.clone())
    }
}

impl<S, M, E> fmt::Debug for ContramapSink<S, M, E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ContramapSink")
            .finish_non_exhaustive()
    }
}

/// A Dispatch that converts `T` before invoking an underlying [`EventSink<E>`].
#[must_use = "a ContramapDispatch must be run, submitted, or intentionally discarded"]
pub struct ContramapDispatch<S, M, T, E> {
    sink: S,
    map: M,
    input: T,
    event: PhantomData<fn(E)>,
}

impl<S, M, T, E> Dispatch for ContramapDispatch<S, M, T, E>
where
    S: EventSink<E>,
    M: FnOnce(T) -> E,
{
    fn run(self) -> DispatchOutcome {
        let Self {
            sink, map, input, ..
        } = self;
        sink.send(map(input)).run()
    }
}

impl<S, M, T, E> fmt::Debug for ContramapDispatch<S, M, T, E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ContramapDispatch")
            .finish_non_exhaustive()
    }
}

/// Adapts an event sink to own borrowed inputs before its Dispatch is built.
///
/// Unlike [`ContramapSink`], this conversion runs in [`EventSink::send`]. The
/// resulting Dispatch is exactly the underlying event sink's Dispatch and therefore
/// does not retain the input borrow.
pub struct OwningSink<S> {
    sink: S,
}

impl<S> OwningSink<S> {
    /// Wraps `sink` with a `ToOwned` conversion at the send boundary.
    pub const fn new(sink: S) -> Self {
        Self { sink }
    }
}

impl<'input, T, S> EventSink<&'input T> for OwningSink<S>
where
    T: ToOwned + ?Sized,
    S: EventSink<T::Owned>,
{
    type Dispatch = S::Dispatch;

    fn send(&self, event: &'input T) -> Self::Dispatch {
        self.sink.send(event.to_owned())
    }
}

impl<S> Clone for OwningSink<S>
where
    S: Clone,
{
    fn clone(&self) -> Self {
        Self::new(self.sink.clone())
    }
}

impl<S> fmt::Debug for OwningSink<S> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("OwningSink").finish_non_exhaustive()
    }
}
