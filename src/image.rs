use std::{
    error::Error,
    fmt,
    hash::{Hash, Hasher},
    sync::{Arc, Weak},
};

struct ImageInner {
    width: u32,
    height: u32,
    rgba8: Vec<u8>,
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

/// An immutable, backend-independent sRGB image with straight alpha.
///
/// Cloning an Image only clones its shared handle. Canvas commands retain a
/// handle so the pixel data remains alive as long as it can be drawn.
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
                rgba8,
            }),
        })
    }

    pub fn width(&self) -> u32 {
        self.inner.width
    }

    pub fn height(&self) -> u32 {
        self.inner.height
    }

    pub(crate) fn rgba8(&self) -> &[u8] {
        &self.inner.rgba8
    }

    pub(crate) fn instance_key(&self) -> ImageInstanceKey {
        ImageInstanceKey(Arc::downgrade(&self.inner))
    }
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
        assert_eq!(image.rgba8().as_ptr(), clone.rgba8().as_ptr());
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
}
