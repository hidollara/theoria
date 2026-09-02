//! Typed event sources, sinks, and selectable dispatch routes.
//!
//! An [`EventSink`] is a typed destination. A [`DispatchRoute`] chooses inline
//! execution or delegates an [`EventDispatch`] to an [`EventDispatcher`].
//! [`FifoDispatcher`] provides serialized FIFO dispatch.
//! [`channel`] creates an owning [`EventProvider`] and its
//! subscription-only [`EventSource`]. Converting a borrowed Provider into an
//! [`EventSink`] produces its publication input. Each Source subscription
//! selects its destination Sink and dispatch route independently.

mod bridge;
mod channel;
mod dispatch;
mod dispatcher;
mod handler;
mod sink;

pub use bridge::{BridgeProgress, BridgeSender, EventBridge, bridge};
pub use channel::{EventProvider, EventSource, EventSubscription, channel};
pub use dispatch::{
    DispatchOutcome, DispatchReceipt, DispatchRoute, DispatchStatus, EventDispatch,
    EventDispatcher, SubmissionOutcome,
};
pub use dispatcher::FifoDispatcher;
pub use handler::EventHandler;
pub use sink::EventSink;
