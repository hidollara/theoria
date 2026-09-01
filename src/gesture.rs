//! Semantic gestures built from primitive input observations.
//!
//! Recognizers in this module do not perform hit testing or event routing.
//! They only describe what happened in the coordinate space of their input.
//! Gesture-specific components may then route those completed values without
//! adding routing semantics to the component Runtime itself.

mod click;
mod click_area;
mod click_component;

pub use click::{Click, ClickRecognizer};
pub use click_area::ClickArea;
pub use click_component::ClickGesture;
