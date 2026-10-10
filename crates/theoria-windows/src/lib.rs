//! Win32 Windows, input, and event-loop integration.
//!
//! Window management does not require a GPU backend. The optional `vulkan`
//! feature adds Canvas presentation constructors.

#![cfg(target_os = "windows")]

use theoria_canvas::geometry;
use theoria_component as component;
use theoria_event as event;
use theoria_runtime::{animation, event_loop, input, surface, window};

mod windows;
pub use windows::*;
