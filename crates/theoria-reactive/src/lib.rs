//! Owning reactive values coordinated by [`Reactive`].
//!
//! [`Reactive`] creates State, Computed, and Effect nodes that may depend on one
//! another. The factory does not own those nodes: their public handles and
//! dependency edges do. Dropping an unused acyclic graph therefore releases it
//! without a separate scope or disposal operation.
//!
//! State changes push dirtiness through the dependency graph. Computed results
//! are evaluated and cached on demand, and Effect execution remains explicit.
//!
//! Each independently created Reactive coordinates its own values, and clones
//! share that coordination. A mutation batch excludes evaluation through the
//! same Reactive; one top-level evaluation excludes mutation and another
//! top-level evaluation. Computed and Effect callbacks read through
//! [`ReadContext`] so dependencies are tracked within the existing evaluation.
//! Nodes from independently created Reactives cannot share tracked
//! dependencies or one mutation batch.
//!
//! [`State::set`], [`State::update`], [`Computed::snapshot`], and [`Effect::run`]
//! are convenient single-operation entry points. Use [`Reactive::mutate`] when
//! several State changes must share one isolated batch. A batch prevents
//! overlapping evaluation but is not a rollback transaction.
//!
//! Evaluation cycles and user callback panics are not caught. Capturing a node
//! graph through user-provided interior mutability can also create an Rc cycle;
//! ordinary construction from previously created reactive inputs is acyclic.

mod computed;
mod effect;
mod error;
mod evaluation;
mod node;
mod operation;
mod reactive;
mod read;
mod snapshot_state;
mod state;
mod value;
mod write;

pub use computed::Computed;
pub use effect::Effect;
pub use error::ReactiveError;
pub use reactive::Reactive;
pub use read::{Read, ReadContext, Snapshot};
pub use snapshot_state::SnapshotState;
pub use state::State;
pub use value::Value;
pub use write::{StateSetter, WriteContext};
