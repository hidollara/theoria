//! Recognition of semantic gestures from primitive input observations.
//!
//! Recognizers in this module do not perform hit testing or event routing.
//! They only describe what happened in the coordinate space of their input.

mod click;
mod click_component;

pub use click::{Click, ClickRecognizer};
pub use click_component::ClickGesture;
