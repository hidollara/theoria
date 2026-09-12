//! Internal contract between semantic Windows and platform implementations.

use crate::{
    PresentationTarget,
    input::{pointer::PointerEvent, scroll::ScrollEvent},
    surface::{SurfaceEvent, SurfaceSize},
};

/// Platform input translated into Theoria's Window vocabulary.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum WindowBackendEvent {
    Surface(SurfaceEvent),
    Pointer(PointerEvent),
    Scroll(ScrollEvent),
    CloseRequested,
}

/// Native operations needed by Theoria's platform-independent Window host.
///
/// Implementations retain any required thread affinity. This contract remains
/// private so public Window APIs expose only Theoria types.
pub(crate) trait WindowBackend {
    fn show(&self);
    fn presentation_target(&self) -> PresentationTarget;
    fn size(&self) -> SurfaceSize;
    fn movement_threshold_extents(&self) -> (f32, f32);
    fn request_redraw(&mut self);
    fn has_pending_event(&self) -> bool;
    fn pop_event(&mut self) -> Option<WindowBackendEvent>;
}
