use std::sync::Arc;

use crate::transform::Projective2;

use super::{ColorField, Composition, Coverage};

#[derive(Clone, Debug)]
pub(crate) struct Draw {
    pub(crate) source: ColorField,
    pub(crate) coverage: Coverage,
}

#[derive(Clone, Debug)]
pub(crate) enum CanvasNode {
    Draw(Draw),
    Transform {
        content: Canvas,
        transform: Projective2,
    },
    Composite {
        backdrop: Canvas,
        source: Canvas,
        composition: Composition,
    },
}

/// A shareable drawing value on an unbounded, backend-independent affine plane.
///
/// The Canvas assigns no physical direction to its axes and has no pixel size.
/// A CanvasView gives its coordinates drawing semantics by mapping them into a
/// normalized, top-left-origin View square.
#[derive(Clone, Debug)]
pub struct Canvas {
    pub(super) node: Arc<CanvasNode>,
}

impl Canvas {
    /// Creates a Canvas with no color contribution anywhere.
    pub fn empty() -> Self {
        Self::draw(ColorField::Transparent, Coverage::all())
    }

    /// Forms a Canvas drawing by applying `source` through `coverage`.
    pub fn draw(source: impl Into<ColorField>, coverage: Coverage) -> Self {
        Self {
            node: Arc::new(CanvasNode::Draw(Draw {
                source: source.into(),
                coverage,
            })),
        }
    }

    /// Combines two complete Canvas fields with one explicit composition.
    pub fn composite(self, source: Canvas, composition: Composition) -> Self {
        Self {
            node: Arc::new(CanvasNode::Composite {
                backdrop: self,
                source,
                composition,
            }),
        }
    }

    /// Returns this drawing placed through `transform` in a new Canvas.
    ///
    /// This preserves the drawing as vector-like Canvas structure so a
    /// renderer may distribute the transform into its primitive sources.
    pub fn transformed(self, transform: impl Into<Projective2>) -> Self {
        Self {
            node: Arc::new(CanvasNode::Transform {
                content: self,
                transform: transform.into(),
            }),
        }
    }

    pub(crate) fn node(&self) -> &CanvasNode {
        &self.node
    }
}

impl Default for Canvas {
    fn default() -> Self {
        Self::empty()
    }
}
