use std::{fmt, marker::PhantomData};

use crate::dispatch::{Dispatch, DispatchOutcome};

/// A typed event destination that turns each send into one Dispatch.
///
/// The associated Dispatch preserves its concrete lifetime and auto traits so
/// the selected Dispatcher can decide whether it can execute or retain it.
pub trait EventSink<E> {
    type Dispatch: Dispatch;

    /// Turns `event` into one unexecuted invocation of this event sink.
    fn send(&self, event: E) -> Self::Dispatch;

    /// Accepts `T` by converting it into this sink's `E` during execution.
    fn contramap<T, M>(self, map: M) -> ContramapSink<Self, M, E>
    where
        Self: Clone + Sized,
        M: Fn(T) -> E + Clone,
    {
        ContramapSink::new(self, map)
    }

    /// Accepts a borrowed `T` by converting it into this sink's `E` during
    /// execution.
    ///
    /// This is the higher-ranked counterpart of [`Self::contramap`], allowing
    /// one adapter to accept every lifetime of `&T`.
    fn contramap_borrow<T: ?Sized, M>(self, map: M) -> ContramapSink<Self, M, E>
    where
        Self: Clone + Sized,
        M: for<'event> Fn(&'event T) -> E + Clone,
    {
        ContramapSink::new(self, map)
    }

    /// Accepts `T` by optionally converting it into this sink's `E` during
    /// execution.
    ///
    /// Returning `None` completes the Dispatch normally without invoking
    /// the underlying sink.
    fn contramap_filter<T, M>(self, map: M) -> ContramapFilterSink<Self, M, E>
    where
        Self: Clone + Sized,
        M: Fn(T) -> Option<E> + Clone,
    {
        ContramapFilterSink::new(self, map)
    }

    /// Accepts only values satisfying `predicate`.
    ///
    /// A rejected value completes the Dispatch normally without invoking
    /// the underlying sink.
    fn filter<P>(self, predicate: P) -> FilterSink<Self, P>
    where
        Self: Clone + Sized,
        P: Fn(&E) -> bool + Clone,
    {
        FilterSink::new(self, predicate)
    }

    /// Accepts every `T: Into<E>`.
    fn accepting<T>(self) -> ContramapSink<Self, fn(T) -> E, E>
    where
        Self: Clone + Sized,
        T: Into<E>,
    {
        ContramapSink::new(self, into_event::<T, E>)
    }
}

fn into_event<T: Into<E>, E>(event: T) -> E {
    event.into()
}

impl<E, S> EventSink<E> for &S
where
    S: EventSink<E> + ?Sized,
{
    type Dispatch = S::Dispatch;

    fn send(&self, event: E) -> Self::Dispatch {
        EventSink::send(*self, event)
    }
}

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

/// Adapts an event sink by optionally converting its input during execution.
pub struct ContramapFilterSink<S, M, E> {
    sink: S,
    map: M,
    event: PhantomData<fn(E)>,
}

impl<S, M, E> ContramapFilterSink<S, M, E> {
    /// Wraps `sink` with an optional conversion performed during Dispatch execution.
    pub const fn new(sink: S, map: M) -> Self {
        Self {
            sink,
            map,
            event: PhantomData,
        }
    }
}

impl<T, E, S, M> EventSink<T> for ContramapFilterSink<S, M, E>
where
    S: EventSink<E> + Clone,
    M: Fn(T) -> Option<E> + Clone,
{
    type Dispatch = ContramapFilterDispatch<S, M, T, E>;

    fn send(&self, event: T) -> Self::Dispatch {
        ContramapFilterDispatch {
            sink: self.sink.clone(),
            map: self.map.clone(),
            input: event,
            event: PhantomData,
        }
    }
}

impl<S, M, E> Clone for ContramapFilterSink<S, M, E>
where
    S: Clone,
    M: Clone,
{
    fn clone(&self) -> Self {
        Self::new(self.sink.clone(), self.map.clone())
    }
}

impl<S, M, E> fmt::Debug for ContramapFilterSink<S, M, E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ContramapFilterSink")
            .finish_non_exhaustive()
    }
}

/// A Dispatch that may convert `T` before invoking an underlying event sink.
#[must_use = "a ContramapFilterDispatch must be run, submitted, or intentionally discarded"]
pub struct ContramapFilterDispatch<S, M, T, E> {
    sink: S,
    map: M,
    input: T,
    event: PhantomData<fn(E)>,
}

impl<S, M, T, E> Dispatch for ContramapFilterDispatch<S, M, T, E>
where
    S: EventSink<E>,
    M: FnOnce(T) -> Option<E>,
{
    fn run(self) -> DispatchOutcome {
        let Self {
            sink, map, input, ..
        } = self;
        match map(input) {
            Some(event) => sink.send(event).run(),
            None => DispatchOutcome::Completed,
        }
    }
}

impl<S, M, T, E> fmt::Debug for ContramapFilterDispatch<S, M, T, E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ContramapFilterDispatch")
            .finish_non_exhaustive()
    }
}

/// Adapts an event sink by filtering its input during execution.
pub struct FilterSink<S, P> {
    sink: S,
    predicate: P,
}

impl<S, P> FilterSink<S, P> {
    /// Wraps `sink` with a predicate evaluated during Dispatch execution.
    pub const fn new(sink: S, predicate: P) -> Self {
        Self { sink, predicate }
    }
}

impl<E, S, P> EventSink<E> for FilterSink<S, P>
where
    S: EventSink<E> + Clone,
    P: Fn(&E) -> bool + Clone,
{
    type Dispatch = FilterDispatch<S, P, E>;

    fn send(&self, event: E) -> Self::Dispatch {
        FilterDispatch {
            sink: self.sink.clone(),
            predicate: self.predicate.clone(),
            event,
        }
    }
}

impl<S, P> Clone for FilterSink<S, P>
where
    S: Clone,
    P: Clone,
{
    fn clone(&self) -> Self {
        Self::new(self.sink.clone(), self.predicate.clone())
    }
}

impl<S, P> fmt::Debug for FilterSink<S, P> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("FilterSink").finish_non_exhaustive()
    }
}

/// A Dispatch that filters `E` before invoking an underlying event sink.
#[must_use = "a FilterDispatch must be run, submitted, or intentionally discarded"]
pub struct FilterDispatch<S, P, E> {
    sink: S,
    predicate: P,
    event: E,
}

impl<S, P, E> Dispatch for FilterDispatch<S, P, E>
where
    S: EventSink<E>,
    P: FnOnce(&E) -> bool,
{
    fn run(self) -> DispatchOutcome {
        let Self {
            sink,
            predicate,
            event,
        } = self;
        if predicate(&event) {
            sink.send(event).run()
        } else {
            DispatchOutcome::Completed
        }
    }
}

impl<S, P, E> fmt::Debug for FilterDispatch<S, P, E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FilterDispatch")
            .finish_non_exhaustive()
    }
}

/// Adapts an event sink to own borrowed inputs before its Dispatch is built.
///
/// Unlike [`ContramapSink`], this conversion runs in [`EventSink::send`]. The
/// resulting Dispatch is exactly the underlying event sink's Dispatch and
/// therefore does not retain the input borrow.
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
