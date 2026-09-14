//! Typed event sources, sinks, and dispatchers.
//!
//! A [`Sink`] turns one event into a concrete [`Dispatch`] without choosing
//! where or when it runs. An [`EventDispatcher`] states which Dispatch family
//! it accepts; [`ImmediateDispatcher`] runs any Dispatch synchronously, while
//! [`Dispatcher`] is a cloneable local capability for self-contained work and
//! [`FifoDispatcher`] provides serialized FIFO execution.
//!
//! [`channel`] creates an owning [`EventProvider`] and its non-owning
//! [`ChannelSource`]. A Source connection selects whether publications are
//! borrowed for immediate execution or cloned into self-contained Dispatches.
//! [`bridge`] pairs a local [`EventBridge`] with a thread-safe [`BridgeSink`].

mod bridge;
mod channel;
mod dispatch;
mod dispatcher;
mod handler;
mod sink;
mod sink_adapter;
mod source;

pub use bridge::{BridgeDispatch, BridgeProgress, BridgeSink, EventBridge, bridge};
pub use channel::{
    BorrowedChannelSource, ChannelSource, ClonedChannelSource, EventProvider, channel,
};
pub use dispatch::{
    Dispatch, DispatchOutcome, DispatchReceipt, DispatchStatus, Dispatcher, EventDispatch,
    EventDispatcher, ImmediateDispatcher, SubmissionOutcome, TrackedDispatch,
};
pub use dispatcher::FifoDispatcher;
pub use handler::EventHandler;
pub use sink::{BorrowingEventSink, EventSink, Sink};
pub use sink_adapter::{ContramapDispatch, ContramapSink, OwningSink};
pub use source::{EventConnection, EventSource, Subscription};
