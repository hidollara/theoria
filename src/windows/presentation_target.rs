use crate::surface::SurfaceSize;

/// Raw Win32 handles used to create a presentation surface.
///
/// This value does not own the native Window. The Window and its module must
/// remain valid for every renderer resource created from this target.
#[derive(Clone, Copy, Debug)]
pub struct Win32PresentationTarget {
    hwnd: isize,
    hinstance: isize,
    size: SurfaceSize,
}

impl Win32PresentationTarget {
    /// Constructs a target from raw Win32 `HWND` and `HINSTANCE` values.
    ///
    /// # Safety
    ///
    /// `hwnd` must identify a live Window belonging to `hinstance`, and both
    /// handles must remain valid for every presentation resource derived from
    /// this target. The Window must also be used according to the thread rules
    /// of the presentation API that consumes it.
    pub const unsafe fn from_raw(hwnd: isize, hinstance: isize, size: SurfaceSize) -> Self {
        Self {
            hwnd,
            hinstance,
            size,
        }
    }

    /// Drawable client extent observed when this target was created.
    pub const fn size(self) -> SurfaceSize {
        self.size
    }

    pub(crate) const fn raw_handles(self) -> (isize, isize) {
        (self.hwnd, self.hinstance)
    }
}
