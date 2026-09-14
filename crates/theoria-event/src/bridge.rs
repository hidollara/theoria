use std::{
    fmt,
    sync::mpsc::{self, Receiver, Sender, TryRecvError},
};

use crate::{
    channel::{ChannelSource, EventProvider, channel},
    dispatch::{Dispatch, DispatchOutcome},
    sink::Sink,
};

/// Receives events from another execution domain and publishes them locally.
///
/// The Bridge itself belongs to the local event domain. Its associated
/// [`BridgeSink`] and [`BridgeDispatch`] can cross threads. Forwarding is
/// deliberately one event at a time so scheduling, yielding, and wake policy
/// remain with the caller.
pub struct EventBridge<E: Send + 'static> {
    receiver: Receiver<E>,
    provider: EventProvider<E>,
    source: ChannelSource<E>,
}

/// Creates a local event Bridge and its thread-safe Sink.
pub fn bridge<E: Send + 'static>() -> (EventBridge<E>, BridgeSink<E>) {
    let (sender, receiver) = mpsc::channel();
    let (provider, source) = channel();
    (
        EventBridge {
            receiver,
            provider,
            source,
        },
        BridgeSink { sender },
    )
}

impl<E: Send + 'static> EventBridge<E> {
    /// Returns the local Source published by this Bridge.
    pub fn source(&self) -> ChannelSource<E> {
        self.source.clone()
    }

    /// Attempts to forward exactly one received event into the local Source.
    pub fn forward_next(&self) -> BridgeProgress {
        match self.receiver.try_recv() {
            Ok(event) => {
                self.provider.publish(event);
                BridgeProgress::Forwarded
            }
            Err(TryRecvError::Empty) => BridgeProgress::Idle,
            Err(TryRecvError::Disconnected) => BridgeProgress::Closed,
        }
    }
}

impl<E: Send + 'static> fmt::Debug for EventBridge<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EventBridge")
            .finish_non_exhaustive()
    }
}

/// A cloneable Sink for sending values into an [`EventBridge`].
pub struct BridgeSink<E: Send + 'static> {
    sender: Sender<E>,
}

impl<E: Send + 'static> BridgeSink<E> {
    /// Creates one Dispatch that sends `event` across the Bridge boundary.
    pub fn send(&self, event: E) -> BridgeDispatch<E> {
        BridgeDispatch {
            sender: self.sender.clone(),
            event,
        }
    }
}

impl<E: Send + 'static> Sink<E> for BridgeSink<E> {
    type Dispatch = BridgeDispatch<E>;

    fn send(&self, event: E) -> Self::Dispatch {
        BridgeSink::send(self, event)
    }
}

impl<E: Send + 'static> Clone for BridgeSink<E> {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
        }
    }
}

impl<E: Send + 'static> fmt::Debug for BridgeSink<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("BridgeSink").finish_non_exhaustive()
    }
}

/// One executable send across an [`EventBridge`] boundary.
#[must_use = "a BridgeDispatch must be run, submitted, or intentionally discarded"]
pub struct BridgeDispatch<E: Send + 'static> {
    sender: Sender<E>,
    event: E,
}

impl<E: Send + 'static> BridgeDispatch<E> {
    /// Runs this send and returns the event if the Bridge is unavailable.
    pub fn try_run(self) -> Result<(), E> {
        self.sender.send(self.event).map_err(|error| error.0)
    }
}

impl<E: Send + 'static> Dispatch for BridgeDispatch<E> {
    fn run(self) -> DispatchOutcome {
        match self.try_run() {
            Ok(()) => DispatchOutcome::Delivered,
            Err(_) => DispatchOutcome::SinkUnavailable,
        }
    }
}

impl<E: Send + 'static> fmt::Debug for BridgeDispatch<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BridgeDispatch")
            .finish_non_exhaustive()
    }
}

/// The result of attempting to forward one event through an [`EventBridge`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BridgeProgress {
    /// One event was published into the local event domain.
    Forwarded,
    /// No event is currently available, but senders still exist.
    Idle,
    /// No event remains and every sender has been dropped.
    Closed,
}
