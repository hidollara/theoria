mod icc;
mod pixel_format;
mod raster;

pub use pixel_format::{
    AlphaAssociation, AlphaLayout, HasAlpha, NoAlpha, PixelFormat, Rgb8, Rgba8,
};
pub(crate) use raster::RasterImageInstanceKey;
pub use raster::{RasterImage, RasterImageError, RasterPlane};

use crate::color::Srgb;

/// An immutable, straight-alpha, encoded-sRGB RGBA8 image.
pub type Srgba8Image = RasterImage<Rgba8, Srgb>;

/// Validation failures produced while creating an [`Srgba8Image`].
pub type Srgba8ImageError = RasterImageError;
pub use icc::{IccImageError, IccRasterImage};
