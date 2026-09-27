use std::{
    error::Error,
    fmt,
    hash::{Hash, Hasher},
    sync::{Arc, Weak},
};

use crate::color::{ColorSpace, Srgb};

use super::{AlphaAssociation, AlphaLayout, PixelFormat, Rgba8};

/// One typed raster plane with an explicit row stride measured in samples.
#[derive(Clone, Debug)]
pub struct RasterPlane<S> {
    row_stride: usize,
    samples: Vec<S>,
}

impl<S> RasterPlane<S> {
    pub fn new(row_stride: usize, samples: Vec<S>) -> Self {
        Self {
            row_stride,
            samples,
        }
    }

    pub const fn row_stride(&self) -> usize {
        self.row_stride
    }

    pub fn samples(&self) -> &[S] {
        &self.samples
    }
}

struct RasterImageInner<F, P>
where
    F: PixelFormat,
    P: ColorSpace<Model = F::Model>,
{
    identity: Arc<()>,
    width: u32,
    height: u32,
    planes: Vec<RasterPlane<F::Storage>>,
    profile: P,
    alpha: <F::Alpha as AlphaLayout>::Association,
}

/// An immutable typed raster image.
///
/// `F` describes storage, `P` describes the color meaning of decoded
/// coordinates, and `F::Alpha` determines which alpha association can be
/// represented. Cloning shares the immutable raster allocation.
pub struct RasterImage<F, P>
where
    F: PixelFormat,
    P: ColorSpace<Model = F::Model>,
{
    inner: Arc<RasterImageInner<F, P>>,
}

impl<F, P> RasterImage<F, P>
where
    F: PixelFormat,
    P: ColorSpace<Model = F::Model>,
{
    pub fn from_planes(
        width: u32,
        height: u32,
        planes: Vec<RasterPlane<F::Storage>>,
        profile: P,
        alpha: <F::Alpha as AlphaLayout>::Association,
    ) -> Result<Self, RasterImageError> {
        if width == 0 || height == 0 {
            return Err(RasterImageError::Empty { width, height });
        }
        if planes.len() != F::PLANES {
            return Err(RasterImageError::InvalidPlaneCount {
                expected: F::PLANES,
                actual: planes.len(),
            });
        }
        F::validate(width, height, &planes)?;
        Ok(Self {
            inner: Arc::new(RasterImageInner {
                identity: Arc::new(()),
                width,
                height,
                planes,
                profile,
                alpha,
            }),
        })
    }

    pub fn width(&self) -> u32 {
        self.inner.width
    }

    pub fn height(&self) -> u32 {
        self.inner.height
    }

    pub fn planes(&self) -> &[RasterPlane<F::Storage>] {
        &self.inner.planes
    }

    pub fn profile(&self) -> &P {
        &self.inner.profile
    }

    pub fn alpha_association(&self) -> <F::Alpha as AlphaLayout>::Association {
        self.inner.alpha
    }

    pub(crate) fn instance_key(&self) -> RasterImageInstanceKey {
        RasterImageInstanceKey(Arc::downgrade(&self.inner.identity))
    }
}

impl RasterImage<Rgba8, Srgb> {
    /// Creates a tightly packed, straight-alpha, encoded-sRGB RGBA8 image.
    pub fn new(width: u32, height: u32, pixels: Vec<u8>) -> Result<Self, RasterImageError> {
        let row_stride = usize::try_from(width)
            .ok()
            .and_then(|width| width.checked_mul(4))
            .ok_or(RasterImageError::DimensionsTooLarge { width, height })?;
        Self::from_planes(
            width,
            height,
            vec![RasterPlane::new(row_stride, pixels)],
            Srgb,
            AlphaAssociation::Straight,
        )
    }

    /// Returns tightly packed straight-alpha sRGBA8 pixels in row-major order.
    pub fn pixels(&self) -> &[u8] {
        self.inner.planes[0].samples()
    }
}

impl<F, P> Clone for RasterImage<F, P>
where
    F: PixelFormat,
    P: ColorSpace<Model = F::Model>,
{
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<F, P> fmt::Debug for RasterImage<F, P>
where
    F: PixelFormat,
    P: ColorSpace<Model = F::Model>,
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RasterImage")
            .field("format", &std::any::type_name::<F>())
            .field("profile", &std::any::type_name::<P>())
            .field("width", &self.width())
            .field("height", &self.height())
            .field("planes", &self.planes().len())
            .finish()
    }
}

/// A non-owning identity used by renderer caches.
pub(crate) struct RasterImageInstanceKey(Weak<()>);

impl RasterImageInstanceKey {
    pub(crate) fn from_owner(owner: &Arc<()>) -> Self {
        Self(Arc::downgrade(owner))
    }

    pub(crate) fn is_alive(&self) -> bool {
        self.0.strong_count() != 0
    }
}

impl Clone for RasterImageInstanceKey {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl PartialEq for RasterImageInstanceKey {
    fn eq(&self, other: &Self) -> bool {
        self.0.ptr_eq(&other.0)
    }
}

impl Eq for RasterImageInstanceKey {}

impl Hash for RasterImageInstanceKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.as_ptr().hash(state);
    }
}

impl fmt::Debug for RasterImageInstanceKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("RasterImageInstanceKey")
            .field(&self.0.as_ptr())
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RasterImageError {
    Empty {
        width: u32,
        height: u32,
    },
    DimensionsTooLarge {
        width: u32,
        height: u32,
    },
    InvalidPlaneCount {
        expected: usize,
        actual: usize,
    },
    InvalidRowStride {
        plane: usize,
        minimum: usize,
        actual: usize,
    },
    MisalignedRowStride {
        plane: usize,
        alignment: usize,
        actual: usize,
    },
    InvalidDataLength {
        plane: usize,
        expected: usize,
        actual: usize,
    },
}

impl fmt::Display for RasterImageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty { width, height } => {
                write!(
                    formatter,
                    "image dimensions must be non-zero, got {width}x{height}"
                )
            }
            Self::DimensionsTooLarge { width, height } => {
                write!(
                    formatter,
                    "image dimensions are too large: {width}x{height}"
                )
            }
            Self::InvalidPlaneCount { expected, actual } => {
                write!(
                    formatter,
                    "image has {actual} planes, but its format requires {expected}"
                )
            }
            Self::InvalidRowStride {
                plane,
                minimum,
                actual,
            } => write!(
                formatter,
                "image plane {plane} has row stride {actual}, but requires at least {minimum} samples"
            ),
            Self::InvalidDataLength {
                plane,
                expected,
                actual,
            } => write!(
                formatter,
                "image plane {plane} has {actual} samples, but requires {expected}"
            ),
            Self::MisalignedRowStride {
                plane,
                alignment,
                actual,
            } => write!(
                formatter,
                "image plane {plane} has row stride {actual}, which is not aligned to its {alignment}-sample pixels"
            ),
        }
    }
}

impl Error for RasterImageError {}

#[cfg(test)]
mod tests {
    use crate::color::DisplayP3;

    use super::*;

    #[test]
    fn clones_share_the_same_logical_image() {
        let image = RasterImage::<Rgba8, Srgb>::new(1, 1, vec![10, 20, 30, 40]).unwrap();
        let clone = image.clone();

        assert_eq!(image.instance_key(), clone.instance_key());
        assert_eq!(image.pixels().as_ptr(), clone.pixels().as_ptr());
    }

    #[test]
    fn one_storage_format_accepts_profiles_with_the_same_model() {
        let image = RasterImage::<Rgba8, DisplayP3>::from_planes(
            1,
            1,
            vec![RasterPlane::new(4, vec![10, 20, 30, 40])],
            DisplayP3,
            AlphaAssociation::Straight,
        )
        .unwrap();

        assert_eq!(image.profile(), &DisplayP3);
    }

    #[test]
    fn rejects_an_invalid_pixel_buffer() {
        let error = RasterImage::<Rgba8, Srgb>::new(2, 2, vec![0; 15])
            .err()
            .unwrap();
        assert_eq!(
            error,
            RasterImageError::InvalidDataLength {
                plane: 0,
                expected: 16,
                actual: 15,
            }
        );
    }

    #[test]
    fn independent_images_with_the_same_pixels_are_distinct_registrations() {
        let first = RasterImage::<Rgba8, Srgb>::new(1, 1, vec![10, 20, 30, 40]).unwrap();
        let second = RasterImage::<Rgba8, Srgb>::new(1, 1, vec![10, 20, 30, 40]).unwrap();

        assert_ne!(first.instance_key(), second.instance_key());
    }

    #[test]
    fn cache_key_does_not_keep_the_image_alive() {
        let key = {
            let image = RasterImage::<Rgba8, Srgb>::new(1, 1, vec![10, 20, 30, 40]).unwrap();
            image.instance_key()
        };

        assert!(key.0.upgrade().is_none());
    }

    #[test]
    fn row_padding_is_part_of_the_validated_raster_storage() {
        let image = RasterImage::<Rgba8, Srgb>::from_planes(
            1,
            2,
            vec![RasterPlane::new(8, vec![0; 16])],
            Srgb,
            AlphaAssociation::Premultiplied,
        )
        .unwrap();

        assert_eq!(image.planes()[0].row_stride(), 8);
        assert_eq!(image.alpha_association(), AlphaAssociation::Premultiplied);
    }

    #[test]
    fn interleaved_rows_end_at_pixel_boundaries() {
        let error = RasterImage::<Rgba8, Srgb>::from_planes(
            1,
            1,
            vec![RasterPlane::new(5, vec![0; 5])],
            Srgb,
            AlphaAssociation::Straight,
        )
        .unwrap_err();

        assert_eq!(
            error,
            RasterImageError::MisalignedRowStride {
                plane: 0,
                alignment: 4,
                actual: 5,
            }
        );
    }
}
