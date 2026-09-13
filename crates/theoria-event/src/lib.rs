//! Typed event sources, sinks, and dispatchers.
//!
//! An [`EventSink`] is a typed destination and can be invoked directly.
//! A [`Dispatcher`] submits owned [`EventDispatch`] values for immediate or
//! deferred execution. [`FifoDispatcher`] provides serialized FIFO dispatch.
//! [`channel`] creates an owning [`EventProvider`] and its
//! subscription-only [`EventSource`]. Converting a borrowed Provider into an
//! [`EventSink`] produces its publication input. Each Source subscription
//! selects its destination Sink and either direct or dispatched delivery.

mod bridge;
mod channel;
mod dispatch;
mod dispatcher;
mod handler;
mod sink;

pub use bridge::{BridgeProgress, BridgeSender, EventBridge, bridge};
pub use channel::{EventProvider, EventSource, EventSubscription, channel};
pub use dispatch::{
    DispatchOutcome, DispatchReceipt, DispatchStatus, Dispatcher, EventDispatch, EventDispatcher,
    SubmissionOutcome,
};
pub use dispatcher::FifoDispatcher;
pub use handler::EventHandler;
pub use sink::EventSink;
