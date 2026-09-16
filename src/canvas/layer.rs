use std::{
    error::Error,
    fmt,
    hash::{Hash, Hasher},
    sync::{Arc, Weak},
};

use super::{Canvas, CanvasView};

#[derive(Debug)]
struct LayerInner {
    content: Canvas,
    view: CanvasView,
    width: u32,
    height: u32,
}

/// A finite-resolution evaluation boundary embedded in Canvas space.
///
/// A renderer may materialize one Layer into a backend image and reuse that
/// result wherever clones of the Layer occur. The cache itself remains an
/// implementation detail and may be discarded and rebuilt. The Layer is
/// placed back into Canvas space through the same View used to evaluate it;
/// points outside that finite View are transparent.
#[derive(Clone, Debug)]
pub struct Layer {
    inner: Arc<LayerInner>,
}

impl Layer {
    /// Fixes `content` to one View and pixel resolution.
    pub fn new(
        content: Canvas,
        view: CanvasView,
        width: u32,
        height: u32,
    ) -> Result<Self, LayerError> {
        validate_dimensions(width, height)?;
        Ok(Self {
            inner: Arc::new(LayerInner {
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

    pub(crate) fn instance_key(&self) -> LayerInstanceKey {
        LayerInstanceKey(Arc::downgrade(&self.inner))
    }
}

#[derive(Clone)]
pub(crate) struct LayerInstanceKey(Weak<LayerInner>);

impl LayerInstanceKey {
    pub(crate) fn is_alive(&self) -> bool {
        self.0.strong_count() != 0
    }
}

impl PartialEq for LayerInstanceKey {
    fn eq(&self, other: &Self) -> bool {
        self.0.ptr_eq(&other.0)
    }
}

impl Eq for LayerInstanceKey {}

impl Hash for LayerInstanceKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.as_ptr().hash(state);
    }
}

impl fmt::Debug for LayerInstanceKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("LayerInstanceKey")
            .field(&self.0.as_ptr())
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LayerError {
    Empty { width: u32, height: u32 },
    DimensionsTooLarge { width: u32, height: u32 },
}

impl fmt::Display for LayerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty { width, height } => {
                write!(
                    formatter,
                    "layer dimensions must be non-zero, got {width}x{height}"
                )
            }
            Self::DimensionsTooLarge { width, height } => {
                write!(
                    formatter,
                    "layer dimensions are too large: {width}x{height}"
                )
            }
        }
    }
}

impl Error for LayerError {}

fn validate_dimensions(width: u32, height: u32) -> Result<(), LayerError> {
    if width == 0 || height == 0 {
        return Err(LayerError::Empty { width, height });
    }
    width
        .checked_mul(height)
        .ok_or(LayerError::DimensionsTooLarge { width, height })?;
    Ok(())
}
