//! Meaning of pixels written to a renderer's destination image.
//!
//! The GPU attachment format is backend-specific and deliberately separate.

use crate::color::ColorSpace;

/// How the destination image represents alpha.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OutputAlpha {
    /// RGB contains the alpha contribution.
    Premultiplied,
    /// RGB is independent of alpha.
    Straight,
    /// The destination discards alpha after compositing over black.
    Opaque,
}

/// Color and alpha meaning requested for a renderer's destination image.
///
/// This does not choose the physical pixel format. The target's backend chooses
/// that format and the renderer converts and quantizes into it when writing.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct OutputImage {
    pub(crate) space: ColorSpace,
    pub(crate) alpha: OutputAlpha,
}

impl OutputImage {
    pub fn new(space: impl Into<ColorSpace>, alpha: OutputAlpha) -> Self {
        Self {
            space: space.into(),
            alpha,
        }
    }

    pub fn space(&self) -> &ColorSpace {
        &self.space
    }

    pub const fn alpha(&self) -> OutputAlpha {
        self.alpha
    }
}
