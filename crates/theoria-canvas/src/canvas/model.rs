use std::sync::Arc;

use crate::transform::Projective2;

use super::{ColorField, Composition, Coverage};

#[derive(Clone, Debug)]
/// A draw's declared color field and coverage, exposed for renderer inspection.
pub struct Draw {
    pub source: ColorField,
    pub coverage: Coverage,
}

#[derive(Clone, Debug)]
/// The drawing structure inspected by a renderer.
/// Constructing a node does not construct a [`Canvas`]; use its drawing methods.
pub enum CanvasNode {
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
///
/// Nodes can be inspected, but are not a second construction route:
/// ```compile_fail
/// use theoria_canvas::{Canvas, CanvasNode};
/// fn wrap(node: CanvasNode) -> Canvas {
///     node.into()
/// }
/// ```
#[derive(Clone, Debug)]
pub struct Canvas {
    pub(super) node: Arc<CanvasNode>,
}

impl Canvas {
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

    /// Borrows this drawing's node without exposing the shared storage.
    pub fn node(&self) -> &CanvasNode {
        &self.node
    }
}
