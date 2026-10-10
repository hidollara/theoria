//! Platform-independent event-loop coordination and observable host capabilities.

use theoria_component as component;
use theoria_event as event;
use theoria_geometry::geometry;
use theoria_reactive as reactive;

pub mod animation;
mod component_execution;
pub mod event_loop;
pub mod input;
pub mod surface;
pub mod window;
