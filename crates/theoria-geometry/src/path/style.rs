use std::sync::Arc;

/// Determines which regions enclosed by a Path are filled.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FillRule {
    /// A point is inside when the signed winding count around it is non-zero.
    #[default]
    NonZero,
    /// A point is inside when a ray from it crosses an odd number of edges.
    EvenOdd,
}

/// The shape placed at both endpoints of an open Subpath stroke.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LineCap {
    /// Ends exactly at the endpoint.
    #[default]
    Butt,
    /// Extends half a stroke width with a square edge.
    Square,
    /// Extends with a semicircle.
    Round,
}

/// The shape joining consecutive segments of a stroked Path.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LineJoin {
    /// Extends segment edges until they meet at a sharp corner.
    #[default]
    Miter,
    /// Cuts the corner off with a straight edge.
    Bevel,
    /// Connects segment edges with a circular arc.
    Round,
}

/// Alternating painted and skipped distances along each Subpath.
///
/// The first interval is painted. An odd number of intervals repeats twice
/// before the painted/skipped phase returns to its initial state.
#[derive(Clone, Debug, PartialEq)]
pub struct DashPattern {
    intervals: Arc<[f32]>,
    offset: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DashPatternError {
    Empty,
    NonFiniteInterval,
    NonPositiveInterval,
    NonFiniteOffset,
}

impl std::fmt::Display for DashPatternError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => formatter.write_str("a dash pattern must contain an interval"),
            Self::NonFiniteInterval => formatter.write_str("dash intervals must be finite"),
            Self::NonPositiveInterval => {
                formatter.write_str("dash intervals must be greater than zero")
            }
            Self::NonFiniteOffset => formatter.write_str("dash offset must be finite"),
        }
    }
}

impl std::error::Error for DashPatternError {}

impl DashPattern {
    /// Creates a repeating dash pattern and a distance into it at each Subpath start.
    pub fn new(
        intervals: impl IntoIterator<Item = f32>,
        offset: f32,
    ) -> Result<Self, DashPatternError> {
        let intervals: Vec<_> = intervals.into_iter().collect();
        if intervals.is_empty() {
            return Err(DashPatternError::Empty);
        }
        if intervals.iter().any(|interval| !interval.is_finite()) {
            return Err(DashPatternError::NonFiniteInterval);
        }
        if intervals.iter().any(|interval| *interval <= 0.0) {
            return Err(DashPatternError::NonPositiveInterval);
        }
        if !offset.is_finite() {
            return Err(DashPatternError::NonFiniteOffset);
        }
        Ok(Self {
            intervals: intervals.into(),
            offset,
        })
    }

    pub fn intervals(&self) -> &[f32] {
        &self.intervals
    }

    pub const fn offset(&self) -> f32 {
        self.offset
    }
}

/// Geometric options for painting along a Path.
#[derive(Clone, Debug, PartialEq)]
pub struct StrokeStyle {
    width: f32,
    cap: LineCap,
    join: LineJoin,
    dash_pattern: Option<DashPattern>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StrokeStyleError {
    NonFiniteWidth,
    NegativeWidth,
}

impl std::fmt::Display for StrokeStyleError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NonFiniteWidth => formatter.write_str("stroke width must be finite"),
            Self::NegativeWidth => formatter.write_str("stroke width must not be negative"),
        }
    }
}

impl std::error::Error for StrokeStyleError {}

impl StrokeStyle {
    pub fn new(width: f32) -> Result<Self, StrokeStyleError> {
        if !width.is_finite() {
            return Err(StrokeStyleError::NonFiniteWidth);
        }
        if width < 0.0 {
            return Err(StrokeStyleError::NegativeWidth);
        }
        Ok(Self {
            width,
            cap: LineCap::default(),
            join: LineJoin::default(),
            dash_pattern: None,
        })
    }

    pub const fn width(&self) -> f32 {
        self.width
    }

    pub const fn cap(&self) -> LineCap {
        self.cap
    }

    pub const fn join(&self) -> LineJoin {
        self.join
    }

    pub const fn with_cap(mut self, cap: LineCap) -> Self {
        self.cap = cap;
        self
    }

    pub const fn with_join(mut self, join: LineJoin) -> Self {
        self.join = join;
        self
    }

    pub fn with_dash_pattern(mut self, pattern: DashPattern) -> Self {
        self.dash_pattern = Some(pattern);
        self
    }

    pub fn without_dash_pattern(mut self) -> Self {
        self.dash_pattern = None;
        self
    }

    pub fn dash_pattern(&self) -> Option<&DashPattern> {
        self.dash_pattern.as_ref()
    }
}
