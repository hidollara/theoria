//! Typed event sources, sinks, and dispatchers.
//!
//! An [`EventSink`] turns one event into a concrete [`Dispatch`] without choosing
//! where or when it runs. An [`EventDispatcher`] states which Dispatch family
//! it accepts; [`ImmediateDispatcher`] runs any Dispatch synchronously, while
//! [`Dispatcher`] is a cloneable local capability for self-contained work and
//! [`FifoDispatcher`] provides serialized FIFO execution.
//! Concrete sinks retain their Dispatch type and capabilities. Use
//! [`ErasedEventSink`] only where different concrete sinks must be stored behind
//! one type.
//!
//! [`channel`] creates an owning [`EventProvider`] and its non-owning
//! [`ChannelSource`]. A Channel Source lends each publication; an
//! [`OwningSink`] can turn that borrow into a self-contained Dispatch at the
//! send boundary.
//! [`bridge`] pairs a local [`EventBridge`] with a thread-safe [`BridgeSink`].

mod bridge;
mod channel;
mod dispatch;
mod dispatcher;
mod handler;
mod sink;
mod source;
mod target;

pub use bridge::{BridgeDispatch, BridgeProgress, BridgeSink, EventBridge, bridge};
pub use channel::{ChannelSource, EventProvider, channel};
pub use dispatch::{
    Dispatch, DispatchOutcome, DispatchReceipt, DispatchStatus, Dispatcher, EventDispatch,
    EventDispatcher, ImmediateDispatcher, SubmissionOutcome, TrackedDispatch,
};
pub use dispatcher::FifoDispatcher;
pub use handler::EventHandler;
pub use sink::{
    ContramapDispatch, ContramapFilterDispatch, ContramapFilterSink, ContramapSink,
    ErasedEventSink, EventSink, FilterDispatch, FilterSink, OwningSink,
};
pub use source::{EventConnection, EventSource, Subscription};
pub use target::EventTarget;
