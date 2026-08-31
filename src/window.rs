use crate::{
    input::{pointer::PointerEvent, scroll::ScrollEvent},
    surface::SurfaceEvent,
};

pub use crate::windows::window::Window;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WindowEvent {
    Surface(SurfaceEvent),
    Pointer(PointerEvent),
    Scroll(ScrollEvent),
    CloseRequested,
}
