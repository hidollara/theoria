//! Structural lifetime boundaries for event-driven reactive components.
//!
//! A Component retains zero or more event handlers, reactive values, Effects,
//! subscriptions, arbitrary resources, and child Components inside one
//! structural lifetime boundary. Components do not have a distinguished event
//! type: each retained handler exposes its own typed
//! [`theoria_event::EventTarget`]. Event delivery comes from [`theoria_event`],
//! while reactive storage and evaluation come from [`theoria_reactive`]. This
//! crate coordinates those independent primitives without merging them.
//!
//! Mount implementations establish ordinary relationships before returning.
//! Creation and connection methods retain the resources documented by each
//! method, while returned handles preserve the ownership semantics of their
//! underlying event or reactive primitive.
//!
//! [`ComponentInstance`] owns one mounted Component tree and is independent
//! of any particular Dispatcher or application loop.

mod component;
mod effect;
mod instance;
mod node;
mod tree;

pub use component::{Component, MountContext, UpdateContext};
pub use instance::ComponentInstance;
