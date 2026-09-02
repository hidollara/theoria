use std::{
    fmt,
    sync::mpsc::{self, Receiver, Sender, TryRecvError},
};

use crate::channel::{EventProvider, EventSource, channel};

/// Receives events from another execution domain and publishes them locally.
///
/// The Bridge itself belongs to the local event domain. Only its associated
/// [`BridgeSender`] crosses threads. Forwarding is deliberately one event at a
/// time so scheduling, yielding, and wake policy remain with the caller.
pub struct EventBridge<E: Send + 'static> {
    receiver: Receiver<E>,
    provider: EventProvider<E>,
    source: EventSource<E>,
}

/// Creates a local event Bridge and its thread-safe sending capability.
pub fn bridge<E: Send + 'static>() -> (EventBridge<E>, BridgeSender<E>) {
    let (sender, receiver) = mpsc::channel();
    let (provider, source) = channel();
    (
        EventBridge {
            receiver,
            provider,
            source,
        },
        BridgeSender { sender },
    )
}

impl<E: Send + 'static> EventBridge<E> {
    /// Returns the local Source published by this Bridge.
    pub fn source(&self) -> EventSource<E> {
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

/// A cloneable capability for sending values into an [`EventBridge`].
pub struct BridgeSender<E: Send + 'static> {
    sender: Sender<E>,
}

impl<E: Send + 'static> BridgeSender<E> {
    /// Sends one value across the Bridge boundary.
    ///
    /// Failure returns the original value when the receiving Bridge has been
    /// dropped.
    pub fn send(&self, event: E) -> Result<(), E> {
        self.sender.send(event).map_err(|error| error.0)
    }
}

impl<E: Send + 'static> Clone for BridgeSender<E> {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
        }
    }
}

impl<E: Send + 'static> fmt::Debug for BridgeSender<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BridgeSender")
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
