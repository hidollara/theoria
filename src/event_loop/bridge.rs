//! Cross-thread event production attached to a local EventLoop.

use std::{cell::Cell, error::Error, fmt, io, rc::Rc, sync::Arc, time::Instant};

use crate::event::{
    BridgeProgress, BridgeSender, EventBridge, EventSource, bridge as event_bridge,
};

use super::{EventLoopClosed, EventLoopHandle, EventLoopParticipant, EventLoopWaker, Result};

/// Owns the receiving side of a cross-thread event Bridge.
///
/// The Bridge is registered non-owningly with its EventLoop. Dropping it closes
/// its [`EventSource`] and causes existing [`EventLoopBridgeSender`] values to
/// return their unsent events.
pub struct EventLoopBridge<E: Send + 'static> {
    state: Rc<BridgeState<E>>,
}

impl<E: Send + 'static> EventLoopBridge<E> {
    /// Returns the local subscription interface for events crossing this Bridge.
    pub fn source(&self) -> EventSource<E> {
        self.state.bridge.source()
    }
}

impl<E: Send + 'static> fmt::Debug for EventLoopBridge<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EventLoopBridge")
            .finish_non_exhaustive()
    }
}

/// A cloneable, thread-safe capability for sending into an [`EventLoopBridge`].
///
/// A successful send first crosses the Bridge boundary, then wakes its local
/// EventLoop so the event can be published there.
pub struct EventLoopBridgeSender<E: Send + 'static> {
    state: Arc<SenderState<E>>,
}

impl<E: Send + 'static> EventLoopBridgeSender<E> {
    /// Sends one event and wakes the receiving EventLoop.
    pub fn send(&self, event: E) -> std::result::Result<(), EventLoopBridgeSendError<E>> {
        self.state
            .sender
            .as_ref()
            .expect("an EventLoopBridgeSender retains its input capability")
            .send(event)
            .map_err(EventLoopBridgeSendError::BridgeUnavailable)?;
        self.state
            .waker
            .wake()
            .map_err(EventLoopBridgeSendError::WakeFailed)
    }
}

impl<E: Send + 'static> Clone for EventLoopBridgeSender<E> {
    fn clone(&self) -> Self {
        Self {
            state: Arc::clone(&self.state),
        }
    }
}

impl<E: Send + 'static> fmt::Debug for EventLoopBridgeSender<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EventLoopBridgeSender")
            .finish_non_exhaustive()
    }
}

struct SenderState<E: Send + 'static> {
    sender: Option<BridgeSender<E>>,
    waker: EventLoopWaker,
}

impl<E: Send + 'static> Drop for SenderState<E> {
    fn drop(&mut self) {
        // A final Sender disappearing changes the receiving side to Closed.
        // Waking lets the EventLoop observe that transition without polling.
        self.sender.take();
        let _ = self.waker.wake();
    }
}

/// Failure to send an event through an [`EventLoopBridgeSender`].
pub enum EventLoopBridgeSendError<E> {
    /// The Bridge had already been dropped; the event was not consumed.
    BridgeUnavailable(E),
    /// The event crossed the Bridge, but its EventLoop could not be woken.
    ///
    /// The receiving side may still publish the event if driven for another
    /// reason.
    WakeFailed(io::Error),
}

impl<E> fmt::Debug for EventLoopBridgeSendError<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BridgeUnavailable(_) => formatter
                .debug_tuple("BridgeUnavailable")
                .field(&"<event>")
                .finish(),
            Self::WakeFailed(error) => formatter.debug_tuple("WakeFailed").field(error).finish(),
        }
    }
}

impl<E> fmt::Display for EventLoopBridgeSendError<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BridgeUnavailable(_) => formatter.write_str("the event Bridge is unavailable"),
            Self::WakeFailed(_) => {
                formatter.write_str("the receiving EventLoop could not be woken")
            }
        }
    }
}

impl<E: 'static> Error for EventLoopBridgeSendError<E> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::BridgeUnavailable(_) => None,
            Self::WakeFailed(error) => Some(error),
        }
    }
}

pub(super) fn create<E>(
    event_loop: &EventLoopHandle,
) -> std::result::Result<(EventLoopBridge<E>, EventLoopBridgeSender<E>), EventLoopClosed>
where
    E: Send + 'static,
{
    let (bridge, sender) = event_bridge();
    let state = Rc::new(BridgeState {
        bridge,
        closed: Cell::new(false),
    });
    event_loop.register(&state)?;
    Ok((
        EventLoopBridge { state },
        EventLoopBridgeSender {
            state: Arc::new(SenderState {
                sender: Some(sender),
                waker: event_loop.waker(),
            }),
        },
    ))
}

struct BridgeState<E: Send + 'static> {
    bridge: EventBridge<E>,
    closed: Cell<bool>,
}

impl<E: Send + 'static> EventLoopParticipant for BridgeState<E> {
    fn is_closed(&self) -> bool {
        self.closed.get()
    }

    fn keeps_alive(&self) -> bool {
        true
    }

    fn process_ready_work(&self, _now: Instant) -> Result<()> {
        if self.bridge.forward_next() == BridgeProgress::Closed {
            self.closed.set(true);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::Cell, rc::Rc};

    use crate::event::{DispatchRoute, EventHandler, EventSink};

    use super::*;
    use crate::event_loop::EventLoop;

    #[test]
    fn cross_thread_events_are_published_on_the_event_loop_thread() {
        let mut event_loop = EventLoop::new();
        let (bridge, sender) = event_loop.handle().bridge::<u32>().unwrap();
        let received = Rc::new(Cell::new(None));
        let output = Rc::clone(&received);
        let handler = EventHandler::new(move |event: u32| output.set(Some(event)));
        let _subscription = bridge
            .source()
            .subscribe_cloned(EventSink::from(&handler), &DispatchRoute::Inline)
            .expect("the Bridge owns its live Source");

        std::thread::spawn(move || sender.send(42))
            .join()
            .expect("the sending thread exits normally")
            .expect("the live Bridge accepts and wakes for its event");

        event_loop
            .run()
            .expect("the Bridge closes after publishing its final event");
        assert_eq!(received.get(), Some(42));
    }

    #[test]
    fn a_bridge_sender_does_not_keep_its_bridge_alive() {
        let event_loop = EventLoop::new();
        let (bridge, sender) = event_loop.handle().bridge::<u32>().unwrap();
        drop(bridge);

        assert!(matches!(
            sender.send(42),
            Err(EventLoopBridgeSendError::BridgeUnavailable(42))
        ));
    }

    #[test]
    fn a_bridge_sender_is_safe_to_move_between_threads() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<EventLoopBridgeSender<u32>>();
    }
}
