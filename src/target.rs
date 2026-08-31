use crate::surface::SurfaceSize;

/// A top-left-origin, right-and-down target-space rectangle receiving a View.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Viewport {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Viewport {
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub(crate) fn has_finite_positive_size(self) -> bool {
        self.x.is_finite()
            && self.y.is_finite()
            && self.width.is_finite()
            && self.height.is_finite()
            && self.width > 0.0
            && self.height > 0.0
    }
}

#[derive(Clone, Copy, Debug)]
pub struct PresentationTarget {
    native: NativePresentationTarget,
    size: SurfaceSize,
}

#[derive(Clone, Copy, Debug)]
enum NativePresentationTarget {
    Win32 { hwnd: isize, hinstance: isize },
}

impl PresentationTarget {
    pub(crate) const fn win32(hwnd: isize, hinstance: isize, size: SurfaceSize) -> Self {
        Self {
            native: NativePresentationTarget::Win32 { hwnd, hinstance },
            size,
        }
    }

    pub const fn size(self) -> SurfaceSize {
        self.size
    }

    pub(crate) const fn win32_handles(self) -> (isize, isize) {
        match self.native {
            NativePresentationTarget::Win32 { hwnd, hinstance } => (hwnd, hinstance),
        }
    }
}
