use std::{
    error::Error,
    fmt,
    hash::{Hash, Hasher},
    sync::{Arc, Weak},
};

use crate::canvas::{Canvas, CanvasView};

struct ImageInner {
    width: u32,
    height: u32,
    content: ImageContent,
}

pub(crate) enum ImageContent {
    Rgba8(Vec<u8>),
    Canvas { canvas: Canvas, view: CanvasView },
}

/// An instance-identity key used internally by backend caches.
///
/// The Weak handle lets a cache detect that no Canvas or caller retains the
/// logical image without keeping its CPU pixels alive itself.
#[derive(Clone)]
pub(crate) struct ImageInstanceKey(Weak<ImageInner>);

impl PartialEq for ImageInstanceKey {
    fn eq(&self, other: &Self) -> bool {
        self.0.ptr_eq(&other.0)
    }
}

impl Eq for ImageInstanceKey {}

impl Hash for ImageInstanceKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.as_ptr().hash(state);
    }
}

impl fmt::Debug for ImageInstanceKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("ImageInstanceKey")
            .field(&self.0.as_ptr())
            .finish()
    }
}

/// An immutable logical 2D raster resource.
///
/// An Image contains either CPU raster data or a finite Canvas evaluation
/// recipe. Its physical GPU representation is intentionally not part of the
/// public API and may be allocated, uploaded, and cached lazily by each
/// renderer. Cloning only clones the shared logical handle.
#[derive(Clone)]
pub struct Image {
    inner: Arc<ImageInner>,
}

impl fmt::Debug for Image {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Image")
            .field("width", &self.width())
            .field("height", &self.height())
            .field(
                "content",
                &match &self.inner.content {
                    ImageContent::Rgba8(_) => "rgba8",
                    ImageContent::Canvas { .. } => "canvas",
                },
            )
            .finish_non_exhaustive()
    }
}

impl Image {
    pub fn from_rgba8(width: u32, height: u32, rgba8: Vec<u8>) -> Result<Self, ImageError> {
        let pixel_count = width
            .checked_mul(height)
            .ok_or(ImageError::DimensionsTooLarge { width, height })?;
        let expected = usize::try_from(pixel_count)
            .ok()
            .and_then(|count| count.checked_mul(4))
            .ok_or(ImageError::DimensionsTooLarge { width, height })?;
        if width == 0 || height == 0 {
            return Err(ImageError::Empty { width, height });
        }
        if rgba8.len() != expected {
            return Err(ImageError::InvalidDataLength {
                expected,
                actual: rgba8.len(),
            });
        }

        Ok(Self {
            inner: Arc::new(ImageInner {
                width,
                height,
                content: ImageContent::Rgba8(rgba8),
            }),
        })
    }

    /// Declares a finite raster evaluation of an immutable Canvas.
    ///
    /// This does not allocate a GPU image or execute rendering. A renderer
    /// materializes and caches the result when an Image source is needed.
    pub fn from_canvas(
        width: u32,
        height: u32,
        canvas: Canvas,
        view: CanvasView,
    ) -> Result<Self, ImageError> {
        validate_dimensions(width, height)?;
        Ok(Self {
            inner: Arc::new(ImageInner {
                width,
                height,
                content: ImageContent::Canvas { canvas, view },
            }),
        })
    }

    pub fn width(&self) -> u32 {
        self.inner.width
    }

    pub fn height(&self) -> u32 {
        self.inner.height
    }

    pub(crate) fn content(&self) -> &ImageContent {
        &self.inner.content
    }

    pub(crate) fn rgba8(&self) -> Option<&[u8]> {
        match self.content() {
            ImageContent::Rgba8(rgba8) => Some(rgba8),
            ImageContent::Canvas { .. } => None,
        }
    }

    pub(crate) fn instance_key(&self) -> ImageInstanceKey {
        ImageInstanceKey(Arc::downgrade(&self.inner))
    }
}

impl ImageInstanceKey {
    pub(crate) fn is_alive(&self) -> bool {
        self.0.strong_count() != 0
    }
}

fn validate_dimensions(width: u32, height: u32) -> Result<(), ImageError> {
    if width == 0 || height == 0 {
        return Err(ImageError::Empty { width, height });
    }
    width
        .checked_mul(height)
        .ok_or(ImageError::DimensionsTooLarge { width, height })?;
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImageError {
    Empty { width: u32, height: u32 },
    DimensionsTooLarge { width: u32, height: u32 },
    InvalidDataLength { expected: usize, actual: usize },
}

impl fmt::Display for ImageError {
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
            Self::InvalidDataLength { expected, actual } => write!(
                formatter,
                "RGBA8 data has {actual} bytes, but the image requires {expected}"
            ),
        }
    }
}

impl Error for ImageError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clones_share_the_same_logical_image() {
        let image = Image::from_rgba8(1, 1, vec![10, 20, 30, 40]).unwrap();
        let clone = image.clone();

        assert_eq!(image.instance_key(), clone.instance_key());
        assert_eq!(
            image.rgba8().unwrap().as_ptr(),
            clone.rgba8().unwrap().as_ptr()
        );
    }

    #[test]
    fn rejects_an_invalid_pixel_buffer() {
        let error = Image::from_rgba8(2, 2, vec![0; 15]).err().unwrap();
        assert_eq!(
            error,
            ImageError::InvalidDataLength {
                expected: 16,
                actual: 15,
            }
        );
    }

    #[test]
    fn independent_images_with_the_same_pixels_are_distinct_registrations() {
        let first = Image::from_rgba8(1, 1, vec![10, 20, 30, 40]).unwrap();
        let second = Image::from_rgba8(1, 1, vec![10, 20, 30, 40]).unwrap();

        assert_ne!(first.instance_key(), second.instance_key());
    }

    #[test]
    fn cache_key_does_not_keep_the_image_alive() {
        let key = {
            let image = Image::from_rgba8(1, 1, vec![10, 20, 30, 40]).unwrap();
            image.instance_key()
        };

        assert!(key.0.upgrade().is_none());
    }

    #[test]
    fn canvas_image_is_an_unevaluated_raster_recipe() {
        let canvas = Canvas::empty();
        let view = CanvasView::axis_aligned(
            crate::geometry::Point::new(0.0, 0.0),
            crate::geometry::Point::new(8.0, 6.0),
        )
        .unwrap();
        let image = Image::from_canvas(8, 6, canvas.clone(), view).unwrap();

        assert_eq!((image.width(), image.height()), (8, 6));
        let ImageContent::Canvas {
            canvas: producer,
            view: producer_view,
        } = image.content()
        else {
            panic!("expected a Canvas image recipe");
        };
        assert!(matches!(producer.node(), crate::canvas::CanvasNode::Empty));
        assert_eq!(*producer_view, view);
    }
}
