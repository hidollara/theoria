/// The finite extent of a drawable surface in surface pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SurfaceSize {
    pub width: u32,
    pub height: u32,
}

/// An observation concerning one finite drawable surface.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SurfaceEvent {
    Resized(SurfaceSize),
    RedrawRequested,
}
