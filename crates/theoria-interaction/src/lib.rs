//! Hit testing, pointer gesture recognition, and Component input bindings.
//!
//! These operations do not require a native window or GPU backend. Hit testing
//! uses Geometry's optional CPU path flattening support.

use theoria_component as component;
use theoria_event as event;
use theoria_geometry::{geometry, path, transform};
use theoria_runtime::input;

pub mod gesture;
pub mod interaction;

#[cfg(test)]
use theoria_reactive as reactive;
#[cfg(test)]
mod test_support;
