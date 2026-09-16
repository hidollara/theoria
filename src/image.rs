use std::{
    error::Error,
    fmt,
    hash::{Hash, Hasher},
    sync::{Arc, Weak},
};

struct Srgba8ImageInner {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

/// An instance-identity key used internally by backend caches.
///
/// The Weak handle lets a cache detect that no Canvas or caller retains the
/// logical image without keeping its CPU pixels alive itself.
#[derive(Clone)]
pub(crate) struct Srgba8ImageInstanceKey(Weak<Srgba8ImageInner>);

impl PartialEq for Srgba8ImageInstanceKey {
    fn eq(&self, other: &Self) -> bool {
        self.0.ptr_eq(&other.0)
    }
}

impl Eq for Srgba8ImageInstanceKey {}

impl Hash for Srgba8ImageInstanceKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.as_ptr().hash(state);
    }
}

impl fmt::Debug for Srgba8ImageInstanceKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("Srgba8ImageInstanceKey")
            .field(&self.0.as_ptr())
            .finish()
    }
}

/// An immutable straight-alpha sRGBA8 image.
///
/// Its physical GPU representation is intentionally not part of the public
/// API and may be uploaded and cached lazily by each renderer. Cloning only
/// clones the shared logical handle.
#[derive(Clone)]
pub struct Srgba8Image {
    inner: Arc<Srgba8ImageInner>,
}

impl fmt::Debug for Srgba8Image {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Srgba8Image")
            .field("width", &self.width())
            .field("height", &self.height())
            .finish_non_exhaustive()
    }
}

impl Srgba8Image {
    pub fn new(width: u32, height: u32, pixels: Vec<u8>) -> Result<Self, Srgba8ImageError> {
        let pixel_count = width
            .checked_mul(height)
            .ok_or(Srgba8ImageError::DimensionsTooLarge { width, height })?;
        let expected = usize::try_from(pixel_count)
            .ok()
            .and_then(|count| count.checked_mul(4))
            .ok_or(Srgba8ImageError::DimensionsTooLarge { width, height })?;
        if width == 0 || height == 0 {
            return Err(Srgba8ImageError::Empty { width, height });
        }
        if pixels.len() != expected {
            return Err(Srgba8ImageError::InvalidDataLength {
                expected,
                actual: pixels.len(),
            });
        }

        Ok(Self {
            inner: Arc::new(Srgba8ImageInner {
                width,
                height,
                pixels,
            }),
        })
    }

    pub fn width(&self) -> u32 {
        self.inner.width
    }

    pub fn height(&self) -> u32 {
        self.inner.height
    }

    /// Returns the straight-alpha sRGBA8 pixels in row-major order.
    pub fn pixels(&self) -> &[u8] {
        &self.inner.pixels
    }

    pub(crate) fn instance_key(&self) -> Srgba8ImageInstanceKey {
        Srgba8ImageInstanceKey(Arc::downgrade(&self.inner))
    }
}

impl Srgba8ImageInstanceKey {
    pub(crate) fn is_alive(&self) -> bool {
        self.0.strong_count() != 0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Srgba8ImageError {
    Empty { width: u32, height: u32 },
    DimensionsTooLarge { width: u32, height: u32 },
    InvalidDataLength { expected: usize, actual: usize },
}

impl fmt::Display for Srgba8ImageError {
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

impl Error for Srgba8ImageError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clones_share_the_same_logical_image() {
        let image = Srgba8Image::new(1, 1, vec![10, 20, 30, 40]).unwrap();
        let clone = image.clone();

        assert_eq!(image.instance_key(), clone.instance_key());
        assert_eq!(image.pixels().as_ptr(), clone.pixels().as_ptr());
    }

    #[test]
    fn rejects_an_invalid_pixel_buffer() {
        let error = Srgba8Image::new(2, 2, vec![0; 15]).err().unwrap();
        assert_eq!(
            error,
            Srgba8ImageError::InvalidDataLength {
                expected: 16,
                actual: 15,
            }
        );
    }

    #[test]
    fn independent_images_with_the_same_pixels_are_distinct_registrations() {
        let first = Srgba8Image::new(1, 1, vec![10, 20, 30, 40]).unwrap();
        let second = Srgba8Image::new(1, 1, vec![10, 20, 30, 40]).unwrap();

        assert_ne!(first.instance_key(), second.instance_key());
    }

    #[test]
    fn cache_key_does_not_keep_the_image_alive() {
        let key = {
            let image = Srgba8Image::new(1, 1, vec![10, 20, 30, 40]).unwrap();
            image.instance_key()
        };

        assert!(key.0.upgrade().is_none());
    }
}
