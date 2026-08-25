use crate::path::{FillRule, Path, StrokeStyle};

use super::ColorField;

#[derive(Clone, Debug)]
pub(crate) enum CoverageNode {
    All,
    Fill { path: Path, rule: FillRule },
    Stroke { path: Path, style: StrokeStyle },
    AlphaOf(ColorField),
}

/// Describes how strongly a completed blend/composite candidate is applied at
/// each Canvas point.
///
/// Zero preserves the existing destination, one applies the candidate fully,
/// and fractional values represent masks or antialiased sample coverage. Its
/// representation is intentionally private so this semantic does not become a
/// public rendering or expression graph.
#[derive(Clone, Debug)]
pub struct Coverage {
    pub(super) node: CoverageNode,
}

impl Coverage {
    /// Covers the entire unbounded Canvas plane.
    pub fn all() -> Self {
        Self {
            node: CoverageNode::All,
        }
    }

    /// Covers the interior of `path` using [`FillRule::NonZero`].
    pub fn fill(path: Path) -> Self {
        Self::fill_with_rule(path, FillRule::NonZero)
    }

    /// Covers the interior of `path` using `rule`.
    pub fn fill_with_rule(path: Path, rule: FillRule) -> Self {
        Self {
            node: CoverageNode::Fill { path, rule },
        }
    }

    /// Covers the stroked outline of `path`.
    pub fn stroke(path: Path, style: StrokeStyle) -> Self {
        Self {
            node: CoverageNode::Stroke { path, style },
        }
    }

    /// Uses a spatial RGBA source's alpha channel as coverage.
    pub fn from_alpha(source: impl Into<ColorField>) -> Self {
        Self {
            node: CoverageNode::AlphaOf(source.into()),
        }
    }

    pub(crate) const fn node(&self) -> &CoverageNode {
        &self.node
    }
}
