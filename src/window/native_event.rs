use crate::{
    input::{pointer::PointerEvent, scroll::ScrollEvent},
    surface::SurfaceEvent,
};

/// Platform input translated into the Window host's internal vocabulary.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum NativeWindowEvent {
    Surface(SurfaceEvent),
    Pointer(PointerEvent),
    Scroll(ScrollEvent),
    CloseRequested,
}
