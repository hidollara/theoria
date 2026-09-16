use std::sync::Arc;

use crate::transform::Projective2;

use super::{BlendMode, ColorField, Composite, Coverage, Layer};

#[derive(Clone, Debug)]
pub(crate) struct Draw {
    pub(crate) source: ColorField,
    pub(crate) blend_mode: BlendMode,
    pub(crate) composite: Composite,
    pub(crate) coverage: Coverage,
}

#[derive(Clone, Debug)]
pub(crate) enum CanvasNode {
    Empty,
    Draw(Draw),
    Transform {
        content: Canvas,
        transform: Projective2,
    },
    Layer(Layer),
    Composite {
        backdrop: Canvas,
        source: Canvas,
        blend_mode: BlendMode,
        composite: Composite,
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
    /// Creates the transparent additive identity of Canvas composition.
    pub fn empty() -> Self {
        Self {
            node: Arc::new(CanvasNode::Empty),
        }
    }

    /// Forms a Canvas drawing by applying `source` through `coverage`.
    pub fn draw(source: impl Into<ColorField>, coverage: Coverage) -> Self {
        Self::draw_with(source, BlendMode::Normal, Composite::default(), coverage)
    }

    /// Forms one semantic ColorField -> Blend -> Composite -> Coverage operation.
    pub fn draw_with(
        source: impl Into<ColorField>,
        blend_mode: BlendMode,
        composite: Composite,
        coverage: Coverage,
    ) -> Self {
        Self {
            node: Arc::new(CanvasNode::Draw(Draw {
                source: source.into(),
                blend_mode,
                composite,
                coverage,
            })),
        }
    }

    /// Combines this backdrop with `source` using Normal blend and `composite`.
    pub fn composite(self, source: Canvas, composite: Composite) -> Self {
        self.blend_composite(source, BlendMode::Normal, composite)
    }

    /// Combines two complete Canvas fields while preserving source/destination roles.
    pub fn blend_composite(
        self,
        source: Canvas,
        blend_mode: BlendMode,
        composite: Composite,
    ) -> Self {
        Self {
            node: Arc::new(CanvasNode::Composite {
                backdrop: self,
                source,
                blend_mode,
                composite,
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

impl From<Layer> for Canvas {
    fn from(layer: Layer) -> Self {
        Self {
            node: Arc::new(CanvasNode::Layer(layer)),
        }
    }
}
