//! Typed event sources, sinks, and dispatchers.
//!
//! An [`EventSink`] turns one event into a concrete [`Dispatch`] without choosing
//! where or when it runs. A [`Dispatcher`] states which Dispatch family
//! it accepts; [`DispatchQueue`] provides serialized FIFO execution and a
//! cloneable [`QueueDispatcher`] submission capability. Inline execution uses
//! [`Dispatch::run`] directly.
//! Concrete sinks retain their Dispatch type and capabilities. Code that must
//! store heterogeneous sinks chooses and owns its type-erasure boundary.
//!
//! An owning [`Channel`] provides non-owning [`ChannelSource`] and
//! [`ChannelSink`] capabilities. A
//! Channel Source lends each publication; an
//! [`Subscription`] builder can own that publication before constructing
//! a self-contained Dispatch.
//! [`bridge`] pairs a local [`EventBridge`] with a thread-safe [`BridgeSink`].

mod bridge;
mod channel;
mod dispatch;
mod sink;
mod source;

pub use bridge::{BridgeDispatch, BridgeProgress, BridgeSink, EventBridge, bridge};
pub use channel::{Channel, ChannelDispatch, ChannelSink, ChannelSource};
pub use dispatch::{
    Dispatch, DispatchHandle, DispatchOutcome, DispatchQueue, DispatchStatus, Dispatcher,
    DynDispatch, QueueDispatcher, SubmissionOutcome, TrackedDispatch,
};
pub use sink::{
    ContramapDispatch, ContramapFilterDispatch, ContramapFilterSink, ContramapSink, EventSink,
    FilterDispatch, FilterSink, OwningSink,
};
pub use source::{EventSource, OwnedSubscriptionBuilder, Subscription, SubscriptionBuilder};
