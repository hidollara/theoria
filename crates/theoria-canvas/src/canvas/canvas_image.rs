use std::{
    error::Error,
    fmt,
    hash::{Hash, Hasher},
    sync::{Arc, Weak},
};

use super::{Canvas, CanvasView};

#[derive(Debug)]
struct CanvasImageInner {
    content: Canvas,
    view: CanvasView,
    width: u32,
    height: u32,
}

/// A finite, backend-independent image evaluation of a [`Canvas`].
///
/// `CanvasImage` fixes an immutable Canvas value to one [`CanvasView`] and
/// pixel extent. Sampling it therefore crosses an explicit rasterization
/// boundary: transforms applied after conversion operate on the finite image,
/// rather than reevaluating the original Canvas at the final resolution.
///
/// The definition itself contains no pixel buffer or backend resource. A
/// renderer may materialize it lazily, reuse that materialization wherever
/// clones occur, and discard and rebuild the physical representation later.
#[derive(Clone, Debug)]
pub struct CanvasImage {
    inner: Arc<CanvasImageInner>,
}

impl CanvasImage {
    /// Defines a finite image by fixing `content` to one View and pixel extent.
    pub fn new(
        content: Canvas,
        view: CanvasView,
        width: u32,
        height: u32,
    ) -> Result<Self, CanvasImageError> {
        validate_dimensions(width, height)?;
        Ok(Self {
            inner: Arc::new(CanvasImageInner {
                content,
                view,
                width,
                height,
            }),
        })
    }

    pub fn content(&self) -> &Canvas {
        &self.inner.content
    }

    pub fn view(&self) -> CanvasView {
        self.inner.view
    }

    pub fn width(&self) -> u32 {
        self.inner.width
    }

    pub fn height(&self) -> u32 {
        self.inner.height
    }

    pub fn instance_key(&self) -> CanvasImageInstanceKey {
        CanvasImageInstanceKey(Arc::downgrade(&self.inner))
    }
}

#[derive(Clone)]
pub struct CanvasImageInstanceKey(Weak<CanvasImageInner>);

impl CanvasImageInstanceKey {
    pub fn is_alive(&self) -> bool {
        self.0.strong_count() != 0
    }
}

impl PartialEq for CanvasImageInstanceKey {
    fn eq(&self, other: &Self) -> bool {
        self.0.ptr_eq(&other.0)
    }
}

impl Eq for CanvasImageInstanceKey {}

impl Hash for CanvasImageInstanceKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.as_ptr().hash(state);
    }
}

impl fmt::Debug for CanvasImageInstanceKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("CanvasImageInstanceKey")
            .field(&self.0.as_ptr())
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CanvasImageError {
    Empty { width: u32, height: u32 },
    DimensionsTooLarge { width: u32, height: u32 },
}

impl fmt::Display for CanvasImageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty { width, height } => {
                write!(
                    formatter,
                    "CanvasImage dimensions must be non-zero, got {width}x{height}"
                )
            }
            Self::DimensionsTooLarge { width, height } => {
                write!(
                    formatter,
                    "CanvasImage dimensions are too large: {width}x{height}"
                )
            }
        }
    }
}

impl Error for CanvasImageError {}

fn validate_dimensions(width: u32, height: u32) -> Result<(), CanvasImageError> {
    if width == 0 || height == 0 {
        return Err(CanvasImageError::Empty { width, height });
    }
    width
        .checked_mul(height)
        .ok_or(CanvasImageError::DimensionsTooLarge { width, height })?;
    Ok(())
}
