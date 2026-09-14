//! Typed event sources, sinks, and dispatchers.
//!
//! An [`EventSink`] turns one event into a concrete [`Dispatch`] without choosing
//! where or when it runs. A [`Dispatcher`] states which Dispatch family
//! it accepts; [`FifoDispatcher`] provides serialized FIFO execution and a
//! cloneable [`FifoSender`] submission capability. Inline execution uses
//! [`Dispatch::run`] directly.
//! Concrete sinks retain their Dispatch type and capabilities. Code that must
//! store heterogeneous sinks chooses and owns its type-erasure boundary.
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
    Dispatch, DispatchHandle, DispatchOutcome, DispatchStatus, DynDispatch, TrackedDispatch,
};
pub use dispatcher::{Dispatcher, FifoDispatcher, FifoSender, SubmissionOutcome};
pub use handler::EventHandler;
pub use sink::{
    ContramapDispatch, ContramapFilterDispatch, ContramapFilterSink, ContramapSink, EventSink,
    FilterDispatch, FilterSink, OwningSink,
};
pub use source::{EventConnection, EventSource, Subscription};
pub use target::{EventTarget, TargetDispatch};
