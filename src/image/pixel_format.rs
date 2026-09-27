use std::fmt;

use crate::color::{ColorModel, Rgb};

use super::{RasterImageError, RasterPlane};

mod private {
    pub trait Sealed {}
}

/// Describes whether a pixel format stores an alpha channel and which
/// association modes are valid for it.
pub trait AlphaLayout: private::Sealed + Copy + fmt::Debug + 'static {
    type Association: Copy + fmt::Debug + PartialEq + Eq + 'static;
}

/// A pixel format without an alpha channel.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct NoAlpha;

impl private::Sealed for NoAlpha {}

impl AlphaLayout for NoAlpha {
    type Association = ();
}

/// A pixel format with an alpha channel.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct HasAlpha;

impl private::Sealed for HasAlpha {}

impl AlphaLayout for HasAlpha {
    type Association = AlphaAssociation;
}

/// Defines whether stored color coordinates have already been multiplied by
/// their stored alpha value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AlphaAssociation {
    Straight,
    Premultiplied,
}

/// Defines the typed memory layout from which a renderer can decode pixels.
///
/// A format defines storage only. [`crate::color::ColorSpace`] gives the
/// decoded coordinates colorimetric meaning.
pub trait PixelFormat: Copy + fmt::Debug + 'static {
    type Model: ColorModel;
    type Storage: Copy + fmt::Debug + 'static;
    type Alpha: AlphaLayout;

    const PLANES: usize;

    fn validate(
        width: u32,
        height: u32,
        planes: &[RasterPlane<Self::Storage>],
    ) -> Result<(), RasterImageError>;
}

/// Interleaved RGB with one normalized `u8` code value per coordinate.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Rgb8;

impl PixelFormat for Rgb8 {
    type Model = Rgb;
    type Storage = u8;
    type Alpha = NoAlpha;

    const PLANES: usize = 1;

    fn validate(
        width: u32,
        height: u32,
        planes: &[RasterPlane<u8>],
    ) -> Result<(), RasterImageError> {
        validate_interleaved(width, height, planes, 3)
    }
}

/// Interleaved RGBA with one normalized `u8` code value per channel.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Rgba8;

impl PixelFormat for Rgba8 {
    type Model = Rgb;
    type Storage = u8;
    type Alpha = HasAlpha;

    const PLANES: usize = 1;

    fn validate(
        width: u32,
        height: u32,
        planes: &[RasterPlane<u8>],
    ) -> Result<(), RasterImageError> {
        validate_interleaved(width, height, planes, 4)
    }
}

fn validate_interleaved<T>(
    width: u32,
    height: u32,
    planes: &[RasterPlane<T>],
    channels: usize,
) -> Result<(), RasterImageError> {
    if planes.len() != 1 {
        return Err(RasterImageError::InvalidPlaneCount {
            expected: 1,
            actual: planes.len(),
        });
    }

    let minimum_stride = usize::try_from(width)
        .ok()
        .and_then(|width| width.checked_mul(channels))
        .ok_or(RasterImageError::DimensionsTooLarge { width, height })?;
    let plane = &planes[0];
    if plane.row_stride() < minimum_stride {
        return Err(RasterImageError::InvalidRowStride {
            plane: 0,
            minimum: minimum_stride,
            actual: plane.row_stride(),
        });
    }
    if !plane.row_stride().is_multiple_of(channels) {
        return Err(RasterImageError::MisalignedRowStride {
            plane: 0,
            alignment: channels,
            actual: plane.row_stride(),
        });
    }

    let row_count = usize::try_from(height)
        .map_err(|_| RasterImageError::DimensionsTooLarge { width, height })?;
    let expected = plane
        .row_stride()
        .checked_mul(row_count)
        .ok_or(RasterImageError::DimensionsTooLarge { width, height })?;
    if plane.samples().len() != expected {
        return Err(RasterImageError::InvalidDataLength {
            plane: 0,
            expected,
            actual: plane.samples().len(),
        });
    }
    Ok(())
}
