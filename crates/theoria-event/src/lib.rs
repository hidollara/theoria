//! Typed event sources, sinks, and dispatchers.
//!
//! An [`EventSink`] is a typed destination and can be invoked directly.
//! A [`Dispatcher`] submits owned [`EventDispatch`] values for immediate or
//! deferred execution. [`FifoDispatcher`] provides serialized FIFO dispatch.
//! [`channel`] creates an owning [`EventProvider`] and its non-owning
//! [`ChannelSource`]. A Source connection selects whether publications are
//! borrowed for immediate execution or cloned for deferred execution.

mod bridge;
mod channel;
mod dispatch;
mod dispatcher;
mod handler;
mod sink;
mod source;

pub use bridge::{BridgeProgress, BridgeSender, EventBridge, bridge};
pub use channel::{
    BorrowedChannelSource, ChannelSource, ClonedChannelSource, EventProvider, channel,
};
pub use dispatch::{
    Dispatch, DispatchOutcome, DispatchReceipt, DispatchStatus, Dispatcher, EventDispatch,
    EventDispatcher, ImmediateDispatcher, SubmissionOutcome,
};
pub use dispatcher::FifoDispatcher;
pub use handler::EventHandler;
pub use sink::{BorrowingEventSink, DispatchSink, EventSink};
pub use source::{EventConnection, EventSource, Subscription};
