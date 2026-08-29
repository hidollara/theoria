use std::sync::Arc;

use crate::{color::Color, geometry::Point, path::Path};

/// Defines how a gradient color line is evaluated outside its `[0, 1]` span.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GradientExtend {
    #[default]
    Pad,
    Repeat,
    Reflect,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GradientStop {
    offset: f32,
    color: Color,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GradientStopError {
    NonFiniteOffset,
    OffsetOutOfRange,
}

impl std::fmt::Display for GradientStopError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NonFiniteOffset => formatter.write_str("gradient stop offset must be finite"),
            Self::OffsetOutOfRange => {
                formatter.write_str("gradient stop offset must be between 0 and 1")
            }
        }
    }
}

impl std::error::Error for GradientStopError {}

impl GradientStop {
    pub fn new(offset: f32, color: Color) -> Result<Self, GradientStopError> {
        if !offset.is_finite() {
            return Err(GradientStopError::NonFiniteOffset);
        }
        if !(0.0..=1.0).contains(&offset) {
            return Err(GradientStopError::OffsetOutOfRange);
        }
        Ok(Self { offset, color })
    }

    pub const fn offset(self) -> f32 {
        self.offset
    }

    pub const fn color(self) -> Color {
        self.color
    }
}

/// Maps a normalized scalar parameter to an ordered sequence of colors.
#[derive(Clone, Debug, PartialEq)]
pub struct ColorRamp {
    stops: Arc<[GradientStop]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorRampError {
    TooFewStops,
    UnorderedStops,
}

impl std::fmt::Display for ColorRampError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooFewStops => {
                formatter.write_str("a color ramp must contain at least two stops")
            }
            Self::UnorderedStops => {
                formatter.write_str("color ramp stops must be ordered by offset")
            }
        }
    }
}

impl std::error::Error for ColorRampError {}

impl ColorRamp {
    pub fn new(stops: impl IntoIterator<Item = GradientStop>) -> Result<Self, ColorRampError> {
        let stops: Vec<_> = stops.into_iter().collect();
        validate_gradient_stops(&stops).map_err(|error| match error {
            GradientStopsError::TooFew => ColorRampError::TooFewStops,
            GradientStopsError::Unordered => ColorRampError::UnorderedStops,
        })?;
        Ok(Self {
            stops: stops.into(),
        })
    }

    pub fn stops(&self) -> &[GradientStop] {
        &self.stops
    }
}

/// A color field parameterized by normalized arc length along each Subpath.
///
/// Each Subpath independently maps its start to zero and its end to one. At a
/// Canvas point, the nearest position on the Path selects the ramp parameter.
#[derive(Clone, Debug, PartialEq)]
pub struct AlongPathGradient {
    path: Path,
    ramp: ColorRamp,
}

impl AlongPathGradient {
    pub const fn new(path: Path, ramp: ColorRamp) -> Self {
        Self { path, ramp }
    }

    pub const fn path(&self) -> &Path {
        &self.path
    }

    pub const fn ramp(&self) -> &ColorRamp {
        &self.ramp
    }
}

/// A color field parameterized by signed distance across a Path.
///
/// Distance zero lies on the Path. Its sign is determined by the 2D
/// determinant of the local Path direction and the displacement toward the
/// sampled point, so reflecting a Canvas also reflects the gradient.
#[derive(Clone, Debug, PartialEq)]
pub struct AcrossPathGradient {
    path: Path,
    from_distance: f32,
    to_distance: f32,
    ramp: ColorRamp,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AcrossPathGradientError {
    NonFiniteDistance,
    CoincidentDistances,
}

impl std::fmt::Display for AcrossPathGradientError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NonFiniteDistance => formatter.write_str("across-Path distances must be finite"),
            Self::CoincidentDistances => {
                formatter.write_str("across-Path distances must be distinct")
            }
        }
    }
}

impl std::error::Error for AcrossPathGradientError {}

impl AcrossPathGradient {
    pub fn new(
        path: Path,
        from_distance: f32,
        to_distance: f32,
        ramp: ColorRamp,
    ) -> Result<Self, AcrossPathGradientError> {
        if !from_distance.is_finite() || !to_distance.is_finite() {
            return Err(AcrossPathGradientError::NonFiniteDistance);
        }
        if from_distance == to_distance {
            return Err(AcrossPathGradientError::CoincidentDistances);
        }
        Ok(Self {
            path,
            from_distance,
            to_distance,
            ramp,
        })
    }

    pub const fn path(&self) -> &Path {
        &self.path
    }

    pub const fn from_distance(&self) -> f32 {
        self.from_distance
    }

    pub const fn to_distance(&self) -> f32 {
        self.to_distance
    }

    pub const fn ramp(&self) -> &ColorRamp {
        &self.ramp
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct LinearGradient {
    start: Point,
    end: Point,
    stops: Arc<[GradientStop]>,
    extend: GradientExtend,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinearGradientError {
    NonFinitePoint,
    CoincidentPoints,
    TooFewStops,
    UnorderedStops,
}

impl std::fmt::Display for LinearGradientError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NonFinitePoint => formatter.write_str("gradient points must be finite"),
            Self::CoincidentPoints => formatter.write_str("gradient points must be distinct"),
            Self::TooFewStops => formatter.write_str("a gradient must contain at least two stops"),
            Self::UnorderedStops => formatter.write_str("gradient stops must be ordered by offset"),
        }
    }
}

impl std::error::Error for LinearGradientError {}

impl LinearGradient {
    pub fn new(
        start: Point,
        end: Point,
        stops: impl IntoIterator<Item = GradientStop>,
    ) -> Result<Self, LinearGradientError> {
        if !start.is_finite() || !end.is_finite() {
            return Err(LinearGradientError::NonFinitePoint);
        }
        if start == end {
            return Err(LinearGradientError::CoincidentPoints);
        }
        let stops: Vec<_> = stops.into_iter().collect();
        if stops.len() < 2 {
            return Err(LinearGradientError::TooFewStops);
        }
        if stops.windows(2).any(|pair| pair[0].offset > pair[1].offset) {
            return Err(LinearGradientError::UnorderedStops);
        }
        Ok(Self {
            start,
            end,
            stops: stops.into(),
            extend: GradientExtend::Pad,
        })
    }

    pub const fn start(&self) -> Point {
        self.start
    }

    pub const fn end(&self) -> Point {
        self.end
    }

    pub fn stops(&self) -> &[GradientStop] {
        &self.stops
    }

    pub const fn with_extend(mut self, extend: GradientExtend) -> Self {
        self.extend = extend;
        self
    }

    pub const fn extend(&self) -> GradientExtend {
        self.extend
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RadialGradient {
    start_center: Point,
    start_radius: f32,
    end_center: Point,
    end_radius: f32,
    stops: Arc<[GradientStop]>,
    extend: GradientExtend,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RadialGradientError {
    NonFiniteCenter,
    NonFiniteRadius,
    NonPositiveRadius,
    TooFewStops,
    UnorderedStops,
}

impl std::fmt::Display for RadialGradientError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NonFiniteCenter => formatter.write_str("gradient center must be finite"),
            Self::NonFiniteRadius => formatter.write_str("gradient radius must be finite"),
            Self::NonPositiveRadius => {
                formatter.write_str("gradient radius must be greater than zero")
            }
            Self::TooFewStops => formatter.write_str("a gradient must contain at least two stops"),
            Self::UnorderedStops => formatter.write_str("gradient stops must be ordered by offset"),
        }
    }
}

impl std::error::Error for RadialGradientError {}

impl RadialGradient {
    pub fn new(
        center: Point,
        radius: f32,
        stops: impl IntoIterator<Item = GradientStop>,
    ) -> Result<Self, RadialGradientError> {
        if radius <= 0.0 {
            return Err(RadialGradientError::NonPositiveRadius);
        }
        Self::between_circles(center, 0.0, center, radius, stops)
    }

    /// Creates a two-point conical gradient from its signed endpoint radii.
    ///
    /// A negative endpoint radius is a valid parameterization: the rendered
    /// cone uses only parameter values whose interpolated radius is
    /// nonnegative. This is needed to preserve normalized COLRv1 color lines.
    pub fn between_circles(
        start_center: Point,
        start_radius: f32,
        end_center: Point,
        end_radius: f32,
        stops: impl IntoIterator<Item = GradientStop>,
    ) -> Result<Self, RadialGradientError> {
        if !start_center.is_finite() || !end_center.is_finite() {
            return Err(RadialGradientError::NonFiniteCenter);
        }
        if !start_radius.is_finite() || !end_radius.is_finite() {
            return Err(RadialGradientError::NonFiniteRadius);
        }
        let stops: Vec<_> = stops.into_iter().collect();
        validate_gradient_stops(&stops).map_err(|error| match error {
            GradientStopsError::TooFew => RadialGradientError::TooFewStops,
            GradientStopsError::Unordered => RadialGradientError::UnorderedStops,
        })?;
        Ok(Self {
            start_center,
            start_radius,
            end_center,
            end_radius,
            stops: stops.into(),
            extend: GradientExtend::Pad,
        })
    }

    pub const fn center(&self) -> Point {
        self.end_center
    }

    pub const fn radius(&self) -> f32 {
        self.end_radius
    }

    pub const fn start_center(&self) -> Point {
        self.start_center
    }
    pub const fn start_radius(&self) -> f32 {
        self.start_radius
    }
    pub const fn end_center(&self) -> Point {
        self.end_center
    }
    pub const fn end_radius(&self) -> f32 {
        self.end_radius
    }

    pub fn stops(&self) -> &[GradientStop] {
        &self.stops
    }

    pub const fn with_extend(mut self, extend: GradientExtend) -> Self {
        self.extend = extend;
        self
    }
    pub const fn extend(&self) -> GradientExtend {
        self.extend
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ConicGradient {
    center: Point,
    start_angle: f32,
    end_angle: f32,
    stops: Arc<[GradientStop]>,
    extend: GradientExtend,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConicGradientError {
    NonFiniteCenter,
    NonFiniteStartAngle,
    NonFiniteEndAngle,
    TooFewStops,
    UnorderedStops,
}

impl std::fmt::Display for ConicGradientError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NonFiniteCenter => formatter.write_str("gradient center must be finite"),
            Self::NonFiniteStartAngle => formatter.write_str("gradient start angle must be finite"),
            Self::NonFiniteEndAngle => formatter.write_str("gradient end angle must be finite"),
            Self::TooFewStops => formatter.write_str("a gradient must contain at least two stops"),
            Self::UnorderedStops => formatter.write_str("gradient stops must be ordered by offset"),
        }
    }
}

impl std::error::Error for ConicGradientError {}

impl ConicGradient {
    /// Creates a full-turn angular gradient beginning at `start_angle`.
    ///
    /// Angles are radians from the positive x-axis in Canvas coordinates. The
    /// sign follows the orientation of those coordinates rather than imposing
    /// a screen-space clockwise or counter-clockwise direction.
    pub fn new(
        center: Point,
        start_angle: f32,
        stops: impl IntoIterator<Item = GradientStop>,
    ) -> Result<Self, ConicGradientError> {
        Self::sweeping(
            center,
            start_angle,
            start_angle + std::f32::consts::TAU,
            stops,
        )
    }

    /// Creates a partial angular gradient on the absolute `[0, TAU)` turn.
    /// Values outside the start/end interval are resolved by `GradientExtend`.
    pub fn sweeping(
        center: Point,
        start_angle: f32,
        end_angle: f32,
        stops: impl IntoIterator<Item = GradientStop>,
    ) -> Result<Self, ConicGradientError> {
        if !center.is_finite() {
            return Err(ConicGradientError::NonFiniteCenter);
        }
        if !start_angle.is_finite() {
            return Err(ConicGradientError::NonFiniteStartAngle);
        }
        if !end_angle.is_finite() {
            return Err(ConicGradientError::NonFiniteEndAngle);
        }
        let stops: Vec<_> = stops.into_iter().collect();
        validate_gradient_stops(&stops).map_err(|error| match error {
            GradientStopsError::TooFew => ConicGradientError::TooFewStops,
            GradientStopsError::Unordered => ConicGradientError::UnorderedStops,
        })?;
        Ok(Self {
            center,
            start_angle,
            end_angle,
            stops: stops.into(),
            extend: GradientExtend::Pad,
        })
    }

    pub const fn center(&self) -> Point {
        self.center
    }

    pub const fn start_angle(&self) -> f32 {
        self.start_angle
    }

    pub const fn end_angle(&self) -> f32 {
        self.end_angle
    }

    pub fn stops(&self) -> &[GradientStop] {
        &self.stops
    }

    pub const fn with_extend(mut self, extend: GradientExtend) -> Self {
        self.extend = extend;
        self
    }
    pub const fn extend(&self) -> GradientExtend {
        self.extend
    }
}

enum GradientStopsError {
    TooFew,
    Unordered,
}

fn validate_gradient_stops(stops: &[GradientStop]) -> Result<(), GradientStopsError> {
    if stops.len() < 2 {
        return Err(GradientStopsError::TooFew);
    }
    if stops.windows(2).any(|pair| pair[0].offset > pair[1].offset) {
        return Err(GradientStopsError::Unordered);
    }
    Ok(())
}

/// A Canvas position carrying a color for mesh interpolation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GradientPoint {
    position: Point,
    color: Color,
}

impl GradientPoint {
    pub const fn new(position: Point, color: Color) -> Self {
        Self { position, color }
    }

    pub const fn position(self) -> Point {
        self.position
    }

    pub const fn color(self) -> Color {
        self.color
    }
}

/// A two-dimensional color field assembled from colored triangular patches.
#[derive(Clone, Debug, PartialEq)]
pub struct MeshGradient {
    triangles: Arc<[[GradientPoint; 3]]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeshGradientError {
    NonFinitePoint,
    DegenerateTriangle,
}

impl std::fmt::Display for MeshGradientError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NonFinitePoint => formatter.write_str("mesh gradient points must be finite"),
            Self::DegenerateTriangle => {
                formatter.write_str("mesh gradient triangle must have a non-zero area")
            }
        }
    }
}

impl std::error::Error for MeshGradientError {}

impl MeshGradient {
    pub fn from_triangle(points: [GradientPoint; 3]) -> Result<Self, MeshGradientError> {
        if !points.iter().all(|point| point.position.is_finite()) {
            return Err(MeshGradientError::NonFinitePoint);
        }
        let area = (points[1].position - points[0].position).x
            * (points[2].position - points[0].position).y
            - (points[1].position - points[0].position).y
                * (points[2].position - points[0].position).x;
        if !area.is_finite() || area == 0.0 {
            return Err(MeshGradientError::DegenerateTriangle);
        }
        Ok(Self {
            triangles: Arc::new([points]),
        })
    }

    pub(crate) fn triangles(&self) -> &[[GradientPoint; 3]] {
        &self.triangles
    }
}
