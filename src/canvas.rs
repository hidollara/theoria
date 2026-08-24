use std::sync::Arc;

use crate::{
    geometry::Point,
    image::Image,
    math::{Matrix2, Vector2},
    path::Path,
    transform::{Affine2, Projective2},
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CanvasView {
    /// Maps Canvas coordinates into the normalized, top-left-origin unit
    /// square whose bottom-right corner is `(1, 1)`.
    pub canvas_to_view: Projective2,
}

impl CanvasView {
    pub const fn new(canvas_to_view: Projective2) -> Self {
        Self { canvas_to_view }
    }

    /// Defines an affine mapping from Canvas points to the normalized View.
    ///
    /// `p00`, `p10`, and `p01` are the Canvas-space images of normalized View
    /// coordinates `(0,0)`, `(1,0)`, and `(0,1)`, respectively.
    pub fn affine(p00: Point, p10: Point, p01: Point) -> Option<Self> {
        if !p00.is_finite() || !p10.is_finite() || !p01.is_finite() {
            return None;
        }

        let x_axis = p10 - p00;
        let y_axis = p01 - p00;

        let view_to_canvas = Affine2::new(
            Matrix2::from_columns(x_axis, y_axis),
            Vector2::new(p00.x, p00.y),
        );
        let canvas_to_view = view_to_canvas.try_inverse()?;
        Some(Self::new(Projective2::from(canvas_to_view)))
    }

    /// Maps two axis-aligned Canvas corners to normalized View coordinates
    /// `(0,0)` and `(1,1)`. Their order determines both View-axis directions.
    pub fn axis_aligned(p00: Point, p11: Point) -> Option<Self> {
        let diagonal = p11 - p00;
        Self::affine(
            p00,
            p00 + Vector2::new(diagonal.x, 0.0),
            p00 + Vector2::new(0.0, diagonal.y),
        )
    }

    /// Maps the four Canvas points corresponding to View coordinates `(0,0)`,
    /// `(1,0)`, `(1,1)`, `(0,1)` onto the normalized View square.
    pub fn projective(p00: Point, p10: Point, p11: Point, p01: Point) -> Option<Self> {
        let view_to_canvas = Projective2::from_unit_square_to_quad([p00, p10, p11, p01])?;
        Some(Self::new(view_to_canvas.try_inverse()?))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    /// Straight-alpha, linear-sRGB components used for rendering calculations.
    red: f32,
    green: f32,
    blue: f32,
    alpha: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorError {
    NonFiniteComponent,
    ComponentOutOfRange,
}

impl std::fmt::Display for ColorError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NonFiniteComponent => formatter.write_str("color components must be finite"),
            Self::ComponentOutOfRange => {
                formatter.write_str("sRGB and alpha components must be between 0 and 1")
            }
        }
    }
}

impl std::error::Error for ColorError {}

impl Color {
    /// Creates an opaque color from normalized sRGB components.
    ///
    /// Returns an error for non-finite components or values outside `[0, 1]`.
    pub fn rgb(red: f32, green: f32, blue: f32) -> Result<Self, ColorError> {
        Self::rgba(red, green, blue, 1.0)
    }

    /// Creates a straight-alpha color from normalized sRGB and linear alpha.
    ///
    /// Returns an error for non-finite components or values outside `[0, 1]`.
    pub fn rgba(red: f32, green: f32, blue: f32, alpha: f32) -> Result<Self, ColorError> {
        validate_srgba([red, green, blue, alpha])?;
        Ok(Self::from_valid_srgba(red, green, blue, alpha))
    }

    /// Creates an opaque color from 8-bit sRGB components.
    pub fn rgb8(red: u8, green: u8, blue: u8) -> Self {
        Self::rgba8(red, green, blue, u8::MAX)
    }

    /// Creates a straight-alpha color from 8-bit sRGB and linear alpha.
    pub fn rgba8(red: u8, green: u8, blue: u8, alpha: u8) -> Self {
        const BYTE_TO_UNIT: f32 = 1.0 / 255.0;
        Self::from_valid_srgba(
            f32::from(red) * BYTE_TO_UNIT,
            f32::from(green) * BYTE_TO_UNIT,
            f32::from(blue) * BYTE_TO_UNIT,
            f32::from(alpha) * BYTE_TO_UNIT,
        )
    }

    fn from_valid_srgba(red: f32, green: f32, blue: f32, alpha: f32) -> Self {
        Self {
            red: srgb_to_linear(red),
            green: srgb_to_linear(green),
            blue: srgb_to_linear(blue),
            alpha,
        }
    }

    pub(crate) const fn to_premultiplied_array(self) -> [f32; 4] {
        [
            self.red * self.alpha,
            self.green * self.alpha,
            self.blue * self.alpha,
            self.alpha,
        ]
    }
}

fn srgb_to_linear(component: f32) -> f32 {
    if component <= 0.04045 {
        component / 12.92
    } else {
        ((component + 0.055) / 1.055).powf(2.4)
    }
}

fn validate_srgba(components: [f32; 4]) -> Result<(), ColorError> {
    if components.iter().any(|component| !component.is_finite()) {
        return Err(ColorError::NonFiniteComponent);
    }
    if components
        .iter()
        .any(|component| !(0.0..=1.0).contains(component))
    {
        return Err(ColorError::ComponentOutOfRange);
    }
    Ok(())
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
}

#[derive(Clone, Debug, PartialEq)]
pub struct RadialGradient {
    center: Point,
    radius: f32,
    stops: Arc<[GradientStop]>,
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
        if !center.is_finite() {
            return Err(RadialGradientError::NonFiniteCenter);
        }
        if !radius.is_finite() {
            return Err(RadialGradientError::NonFiniteRadius);
        }
        if radius <= 0.0 {
            return Err(RadialGradientError::NonPositiveRadius);
        }
        let stops: Vec<_> = stops.into_iter().collect();
        validate_gradient_stops(&stops).map_err(|error| match error {
            GradientStopsError::TooFew => RadialGradientError::TooFewStops,
            GradientStopsError::Unordered => RadialGradientError::UnorderedStops,
        })?;
        Ok(Self {
            center,
            radius,
            stops: stops.into(),
        })
    }

    pub const fn center(&self) -> Point {
        self.center
    }

    pub const fn radius(&self) -> f32 {
        self.radius
    }

    pub fn stops(&self) -> &[GradientStop] {
        &self.stops
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ConicGradient {
    center: Point,
    start_angle: f32,
    stops: Arc<[GradientStop]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConicGradientError {
    NonFiniteCenter,
    NonFiniteStartAngle,
    TooFewStops,
    UnorderedStops,
}

impl std::fmt::Display for ConicGradientError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NonFiniteCenter => formatter.write_str("gradient center must be finite"),
            Self::NonFiniteStartAngle => formatter.write_str("gradient start angle must be finite"),
            Self::TooFewStops => formatter.write_str("a gradient must contain at least two stops"),
            Self::UnorderedStops => formatter.write_str("gradient stops must be ordered by offset"),
        }
    }
}

impl std::error::Error for ConicGradientError {}

impl ConicGradient {
    pub fn new(
        center: Point,
        start_angle: f32,
        stops: impl IntoIterator<Item = GradientStop>,
    ) -> Result<Self, ConicGradientError> {
        if !center.is_finite() {
            return Err(ConicGradientError::NonFiniteCenter);
        }
        if !start_angle.is_finite() {
            return Err(ConicGradientError::NonFiniteStartAngle);
        }
        let stops: Vec<_> = stops.into_iter().collect();
        validate_gradient_stops(&stops).map_err(|error| match error {
            GradientStopsError::TooFew => ConicGradientError::TooFewStops,
            GradientStopsError::Unordered => ConicGradientError::UnorderedStops,
        })?;
        Ok(Self {
            center,
            start_angle,
            stops: stops.into(),
        })
    }

    pub const fn center(&self) -> Point {
        self.center
    }

    pub const fn start_angle(&self) -> f32 {
        self.start_angle
    }

    pub fn stops(&self) -> &[GradientStop] {
        &self.stops
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

/// Filtering used when an image Source is sampled between texels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ImageSampling {
    Nearest,
    #[default]
    Linear,
}

/// Defines an image Source outside its normalized image extent.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ImageExtend {
    /// Produces transparent black outside the image.
    #[default]
    Transparent,
    /// Repeats the nearest edge texel.
    Clamp,
    /// Tiles the image with period one.
    Repeat,
    /// Tiles the image while reflecting every other copy.
    Mirror,
}

/// A spatial RGBA Source backed by an immutable image.
///
/// Image coordinates use the image's top-left-origin pixel space. The mapping
/// converts Canvas coordinates into that space before normalized sampling.
#[derive(Clone, Debug)]
pub struct ImagePattern {
    image: Image,
    canvas_to_image: Projective2,
    sampling: ImageSampling,
    extend_x: ImageExtend,
    extend_y: ImageExtend,
}

impl ImagePattern {
    /// Creates a pattern whose image pixel coordinates equal Canvas coordinates.
    pub const fn new(image: Image) -> Self {
        Self {
            image,
            canvas_to_image: Projective2::IDENTITY,
            sampling: ImageSampling::Linear,
            extend_x: ImageExtend::Transparent,
            extend_y: ImageExtend::Transparent,
        }
    }

    /// Places image pixel coordinates into Canvas space.
    pub fn placed(image: Image, image_to_canvas: impl Into<Projective2>) -> Option<Self> {
        Some(Self::new(image).with_canvas_to_image(image_to_canvas.into().try_inverse()?))
    }

    pub const fn with_canvas_to_image(mut self, canvas_to_image: Projective2) -> Self {
        self.canvas_to_image = canvas_to_image;
        self
    }

    pub const fn with_sampling(mut self, sampling: ImageSampling) -> Self {
        self.sampling = sampling;
        self
    }

    pub const fn with_extend(mut self, extend: ImageExtend) -> Self {
        self.extend_x = extend;
        self.extend_y = extend;
        self
    }

    pub const fn with_extend_x(mut self, extend: ImageExtend) -> Self {
        self.extend_x = extend;
        self
    }

    pub const fn with_extend_y(mut self, extend: ImageExtend) -> Self {
        self.extend_y = extend;
        self
    }

    pub const fn image(&self) -> &Image {
        &self.image
    }

    pub const fn canvas_to_image(&self) -> Projective2 {
        self.canvas_to_image
    }

    pub const fn sampling(&self) -> ImageSampling {
        self.sampling
    }

    pub const fn extend_x(&self) -> ImageExtend {
        self.extend_x
    }

    pub const fn extend_y(&self) -> ImageExtend {
        self.extend_y
    }
}

/// A backend-independent RGBA field over the unbounded Canvas plane.
///
/// Alpha belongs to the source itself (for example an image's alpha or a
/// translucent gradient); it is not the amount by which a draw is applied.
#[derive(Clone, Debug)]
pub enum Source {
    Solid(Color),
    LinearGradient(LinearGradient),
    RadialGradient(RadialGradient),
    ConicGradient(ConicGradient),
    AlongPathGradient(AlongPathGradient),
    AcrossPathGradient(AcrossPathGradient),
    MeshGradient(MeshGradient),
    Image(ImagePattern),
    /// Evaluates a child Canvas as one spatial RGBA source.
    Canvas(Canvas),
}

impl From<Image> for Source {
    fn from(image: Image) -> Self {
        Self::Image(ImagePattern::new(image))
    }
}

impl From<Color> for Source {
    fn from(color: Color) -> Self {
        Self::Solid(color)
    }
}

impl From<LinearGradient> for Source {
    fn from(gradient: LinearGradient) -> Self {
        Self::LinearGradient(gradient)
    }
}

impl From<RadialGradient> for Source {
    fn from(gradient: RadialGradient) -> Self {
        Self::RadialGradient(gradient)
    }
}

impl From<ConicGradient> for Source {
    fn from(gradient: ConicGradient) -> Self {
        Self::ConicGradient(gradient)
    }
}

impl From<AlongPathGradient> for Source {
    fn from(gradient: AlongPathGradient) -> Self {
        Self::AlongPathGradient(gradient)
    }
}

impl From<AcrossPathGradient> for Source {
    fn from(gradient: AcrossPathGradient) -> Self {
        Self::AcrossPathGradient(gradient)
    }
}

impl From<MeshGradient> for Source {
    fn from(gradient: MeshGradient) -> Self {
        Self::MeshGradient(gradient)
    }
}

impl From<ImagePattern> for Source {
    fn from(pattern: ImagePattern) -> Self {
        Self::Image(pattern)
    }
}

impl From<Canvas> for Source {
    fn from(canvas: Canvas) -> Self {
        Self::Canvas(canvas)
    }
}

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

#[derive(Clone, Debug)]
pub(crate) enum CoverageNode {
    All,
    Fill { path: Path, rule: FillRule },
    Stroke { path: Path, style: StrokeStyle },
    SourceAlpha(Source),
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
    node: CoverageNode,
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
    pub fn from_alpha(source: impl Into<Source>) -> Self {
        Self {
            node: CoverageNode::SourceAlpha(source.into()),
        }
    }

    pub(crate) const fn node(&self) -> &CoverageNode {
        &self.node
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Draw {
    pub(crate) source: Source,
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
    Composite {
        backdrop: Canvas,
        source: Canvas,
        blend_mode: BlendMode,
        composite: Composite,
    },
}

/// Determines the RGB color in regions where source and destination overlap.
///
/// Blend modes do not define alpha, compositing, or geometric coverage.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum BlendMode {
    #[default]
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    HardLight,
    SoftLight,
    Difference,
    Exclusion,
    Hue,
    Saturation,
    Color,
    Luminosity,
}

impl BlendMode {
    /// All blend modes defined by W3C Compositing and Blending Level 1.
    pub const ALL: [Self; 16] = [
        Self::Normal,
        Self::Multiply,
        Self::Screen,
        Self::Overlay,
        Self::Darken,
        Self::Lighten,
        Self::ColorDodge,
        Self::ColorBurn,
        Self::HardLight,
        Self::SoftLight,
        Self::Difference,
        Self::Exclusion,
        Self::Hue,
        Self::Saturation,
        Self::Color,
        Self::Luminosity,
    ];
}

/// Determines how source and destination RGBA values become a candidate result.
///
/// The region-selecting variants are the standard Porter-Duff operators.
/// Coverage is applied after this operation. `Plus` adds premultiplied
/// compositing values, including alpha, without imposing a semantic clamp.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Composite {
    /// Produces transparent black regardless of either operand.
    Clear,
    /// Replaces the backdrop with the source.
    Copy,
    /// Keeps the backdrop and ignores the source.
    Destination,
    /// Places the source over the backdrop using premultiplied alpha.
    #[default]
    SourceOver,
    /// Places the backdrop over the source.
    DestinationOver,
    /// Keeps the source only where the backdrop has coverage.
    SourceIn,
    /// Keeps the backdrop only where the source has coverage.
    DestinationIn,
    /// Keeps the source only where the backdrop is transparent.
    SourceOut,
    /// Keeps the backdrop only where the source is transparent.
    DestinationOut,
    /// Places the source inside the backdrop while preserving the backdrop outside it.
    SourceAtop,
    /// Places the backdrop inside the source while preserving the source outside it.
    DestinationAtop,
    /// Keeps portions covered by exactly one operand.
    Xor,
    /// Adds source and destination premultiplied RGBA values.
    Plus,
}

impl Composite {
    pub(crate) const ALL: [Self; 13] = [
        Self::Clear,
        Self::Copy,
        Self::Destination,
        Self::SourceOver,
        Self::DestinationOver,
        Self::SourceIn,
        Self::DestinationIn,
        Self::SourceOut,
        Self::DestinationOut,
        Self::SourceAtop,
        Self::DestinationAtop,
        Self::Xor,
        Self::Plus,
    ];
}

/// Compatibility name retained for code written before `Composite` became the
/// single public identity for built-in composition operations.
pub type CompositeOperator = Composite;

/// A shareable drawing value on an unbounded, backend-independent affine plane.
///
/// The Canvas assigns no physical direction to its axes and has no pixel size.
/// A CanvasView gives its coordinates drawing semantics by mapping them into a
/// normalized, top-left-origin View square.
#[derive(Clone, Debug)]
pub struct Canvas {
    node: Arc<CanvasNode>,
}

impl Canvas {
    /// Creates the transparent additive identity of Canvas composition.
    pub fn empty() -> Self {
        Self {
            node: Arc::new(CanvasNode::Empty),
        }
    }

    /// Forms a Canvas drawing by applying `source` through `coverage`.
    pub fn draw(source: impl Into<Source>, coverage: Coverage) -> Self {
        Self::draw_with(source, BlendMode::Normal, Composite::default(), coverage)
    }

    /// Forms one semantic Source -> Blend -> Composite -> Coverage operation.
    pub fn draw_with(
        source: impl Into<Source>,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn filled_triangle(points: [Point; 3], source: Source) -> Canvas {
        Canvas::draw(source, Coverage::fill(Path::polygon(points)))
    }

    fn assert_point_near(actual: Option<Point>, expected: Point) {
        let actual = actual.unwrap();
        assert!((actual.x - expected.x).abs() < 0.001);
        assert!((actual.y - expected.y).abs() < 0.001);
    }

    #[test]
    fn composes_sources_in_paint_order_without_a_size() {
        let first = [
            Point::new(-10.0, -10.0),
            Point::new(20.0, 30.0),
            Point::new(0.0, 30.0),
        ];
        let second = [
            Point::new(10_000.0, 10_000.0),
            Point::new(10_020.0, 10_030.0),
            Point::new(9_990.0, 10_030.0),
        ];

        let first = filled_triangle(first, Source::Solid(Color::rgb(1.0, 0.0, 0.0).unwrap()));
        let second = filled_triangle(second, Source::Solid(Color::rgb(0.0, 1.0, 0.0).unwrap()));
        let canvas = Canvas::empty()
            .composite(first.clone(), CompositeOperator::SourceOver)
            .composite(second.clone(), CompositeOperator::SourceOver);

        assert!(matches!(canvas.node(), CanvasNode::Composite {
            backdrop,
            source,
            blend_mode: BlendMode::Normal,
            composite: Composite::SourceOver,
        } if Arc::ptr_eq(&source.node, &second.node)
            && matches!(backdrop.node(), CanvasNode::Composite {
                source,
                ..
            } if Arc::ptr_eq(&source.node, &first.node))));
    }

    #[test]
    fn fill_records_non_zero_by_default_and_accepts_even_odd_explicitly() {
        let path = Path::polygon([
            Point::new(0.0, 0.0),
            Point::new(2.0, 0.0),
            Point::new(1.0, 2.0),
        ]);
        let source = Source::Solid(Color::rgb8(255, 255, 255));
        let default_fill = Canvas::draw(source.clone(), Coverage::fill(path.clone()));
        let even_odd = Canvas::draw(source, Coverage::fill_with_rule(path, FillRule::EvenOdd));

        assert!(matches!(
            default_fill.node(),
            CanvasNode::Draw(Draw {
                coverage: Coverage {
                    node: CoverageNode::Fill {
                        rule: FillRule::NonZero,
                        ..
                    }
                },
                ..
            })
        ));
        assert!(matches!(
            even_odd.node(),
            CanvasNode::Draw(Draw {
                coverage: Coverage {
                    node: CoverageNode::Fill {
                        rule: FillRule::EvenOdd,
                        ..
                    }
                },
                ..
            })
        ));
    }

    #[test]
    fn an_image_can_be_used_as_an_unbounded_source_explicitly() {
        let image = Image::from_rgba8(2, 3, vec![255; 2 * 3 * 4]).unwrap();
        let canvas = Canvas::draw(image, Coverage::all());

        let CanvasNode::Draw(Draw {
            source,
            coverage: Coverage {
                node: CoverageNode::All,
            },
            ..
        }) = canvas.node()
        else {
            panic!("the draw must preserve its explicit full coverage");
        };
        assert!(matches!(source, Source::Image(pattern)
                if pattern.image().width() == 2 && pattern.image().height() == 3));
    }

    #[test]
    fn image_pattern_separates_placement_sampling_and_extension() {
        let image = Image::from_rgba8(2, 3, vec![255; 2 * 3 * 4]).unwrap();
        let image_to_canvas = Affine2::from_scale(Vector2::new(2.0, 2.0))
            .then(Affine2::from_translation(Vector2::new(10.0, 20.0)));
        let pattern = ImagePattern::placed(image, image_to_canvas)
            .unwrap()
            .with_sampling(ImageSampling::Nearest)
            .with_extend_x(ImageExtend::Repeat)
            .with_extend_y(ImageExtend::Mirror);

        assert_eq!(pattern.sampling(), ImageSampling::Nearest);
        assert_eq!(pattern.extend_x(), ImageExtend::Repeat);
        assert_eq!(pattern.extend_y(), ImageExtend::Mirror);
        assert_eq!(
            pattern
                .canvas_to_image()
                .try_transform_point(Point::new(16.0, 28.0)),
            Some(Point::new(3.0, 4.0))
        );
    }

    #[test]
    fn image_pattern_defaults_to_a_transparent_linear_source() {
        let image = Image::from_rgba8(1, 1, vec![255; 4]).unwrap();
        let pattern = ImagePattern::new(image);

        assert_eq!(pattern.sampling(), ImageSampling::Linear);
        assert_eq!(pattern.extend_x(), ImageExtend::Transparent);
        assert_eq!(pattern.extend_y(), ImageExtend::Transparent);
        assert_eq!(pattern.canvas_to_image(), Projective2::IDENTITY);
    }

    #[test]
    fn cloned_canvases_diverge_without_changing_the_shared_value() {
        let triangle = [
            Point::new(0.0, 0.0),
            Point::new(1.0, 0.0),
            Point::new(0.0, 1.0),
        ];
        let source = Source::Solid(Color::rgb8(255, 255, 255));
        let base = filled_triangle(triangle, source.clone());
        let extended = base.clone().composite(
            filled_triangle(triangle, source),
            CompositeOperator::SourceOver,
        );

        assert!(matches!(extended.node(), CanvasNode::Composite {
            backdrop,
            ..
        } if Arc::ptr_eq(&backdrop.node, &base.node)));
        assert!(matches!(base.node(), CanvasNode::Draw(_)));
    }

    #[test]
    fn transform_preserves_the_child_canvas_as_a_shared_value() {
        let triangle = [
            Point::new(0.0, 0.0),
            Point::new(1.0, 0.0),
            Point::new(0.0, 1.0),
        ];
        let child = filled_triangle(triangle, Source::Solid(Color::rgb8(255, 255, 255)));
        let child_to_parent =
            Projective2::from_rows([[1.0, 0.0, 20.0], [0.0, 1.0, 30.0], [0.001, 0.0, 1.0]]);
        let transformed = child.clone().transformed(child_to_parent);

        assert!(matches!(transformed.node(), CanvasNode::Transform {
            content,
            transform,
        } if Arc::ptr_eq(&content.node, &child.node) && *transform == child_to_parent));
    }

    #[test]
    fn coverage_pairs_a_lazy_canvas_source_with_a_canvas_alpha_field() {
        let triangle = [
            Point::new(0.0, 0.0),
            Point::new(1.0, 0.0),
            Point::new(0.0, 1.0),
        ];
        let source = Source::Solid(Color::rgb8(255, 255, 255));
        let content = filled_triangle(triangle, source.clone());
        let coverage_canvas = filled_triangle(triangle, source.clone());
        let overlay = filled_triangle(triangle, source);
        let masked = Canvas::draw(
            content.clone(),
            Coverage::from_alpha(coverage_canvas.clone()),
        );
        let canvas = masked.composite(overlay, CompositeOperator::SourceOver);

        assert!(matches!(canvas.node(), CanvasNode::Composite {
            backdrop,
            ..
        } if matches!(backdrop.node(), CanvasNode::Draw(Draw {
            source: Source::Canvas(recorded_content),
            coverage: Coverage {
                node: CoverageNode::SourceAlpha(Source::Canvas(recorded_mask))
            },
            ..
        }) if Arc::ptr_eq(&recorded_content.node, &content.node)
            && Arc::ptr_eq(&recorded_mask.node, &coverage_canvas.node))));
    }

    #[test]
    fn any_source_can_supply_an_alpha_coverage_field() {
        let triangle = Path::polygon([
            Point::new(0.0, 0.0),
            Point::new(1.0, 0.0),
            Point::new(0.0, 1.0),
        ]);
        let content = Canvas::draw(Color::rgb8(255, 255, 255), Coverage::fill(triangle));
        let image = Image::from_rgba8(1, 1, vec![255; 4]).unwrap();
        let masked = Canvas::draw(content, Coverage::from_alpha(ImagePattern::new(image)));

        assert!(matches!(
            masked.node(),
            CanvasNode::Draw(Draw {
                source: Source::Canvas(_),
                coverage: Coverage {
                    node: CoverageNode::SourceAlpha(Source::Image(_))
                },
                ..
            })
        ));
    }

    #[test]
    fn draw_keeps_source_blend_composite_and_coverage_as_distinct_semantics() {
        let source = Source::Solid(Color::rgba(0.8, 0.2, 0.1, 0.4).unwrap());
        let coverage = Coverage::fill(Path::polygon([
            Point::new(0.0, 0.0),
            Point::new(1.0, 0.0),
            Point::new(0.0, 1.0),
        ]));
        let canvas = Canvas::draw_with(
            source.clone(),
            BlendMode::Multiply,
            Composite::Plus,
            coverage.clone(),
        );

        assert!(matches!(
            canvas.node(),
            CanvasNode::Draw(Draw {
                source: Source::Solid(_),
                blend_mode: BlendMode::Multiply,
                composite: Composite::Plus,
                coverage: Coverage {
                    node: CoverageNode::Fill { .. }
                },
            })
        ));
    }

    #[test]
    fn draw_sugar_is_normal_source_over_without_erasing_coverage() {
        let coverage = Coverage::all();
        let canvas = Canvas::draw(Color::rgb8(1, 2, 3), coverage.clone());

        assert!(matches!(
            canvas.node(),
            CanvasNode::Draw(Draw {
                blend_mode: BlendMode::Normal,
                composite: Composite::SourceOver,
                coverage: Coverage {
                    node: CoverageNode::All
                },
                ..
            })
        ));
    }

    #[test]
    fn destination_over_keeps_source_and_backdrop_roles_in_the_ir() {
        let backdrop = Canvas::draw(Color::rgb8(10, 20, 30), Coverage::all());
        let source = Canvas::draw(Color::rgb8(40, 50, 60), Coverage::all());
        let canvas = backdrop.clone().blend_composite(
            source.clone(),
            BlendMode::Screen,
            Composite::DestinationOver,
        );

        assert!(matches!(canvas.node(), CanvasNode::Composite {
            backdrop: recorded_backdrop,
            source: recorded_source,
            blend_mode: BlendMode::Screen,
            composite: Composite::DestinationOver,
        } if Arc::ptr_eq(&recorded_backdrop.node, &backdrop.node)
            && Arc::ptr_eq(&recorded_source.node, &source.node)));
    }

    #[test]
    fn transformed_wraps_the_canvas_without_baking_primitive_coordinates() {
        let triangle = [
            Point::new(0.0, 0.0),
            Point::new(1.0, 0.0),
            Point::new(0.0, 1.0),
        ];
        let transform =
            Projective2::from_rows([[2.0, 0.0, 10.0], [0.0, 3.0, 20.0], [0.0, 0.0, 1.0]]);
        let content = filled_triangle(triangle, Source::Solid(Color::rgb8(255, 255, 255)));
        let transformed = content.clone().transformed(transform);

        assert!(matches!(transformed.node(), CanvasNode::Transform {
            content: recorded,
            transform: recorded_transform,
        } if Arc::ptr_eq(&recorded.node, &content.node) && *recorded_transform == transform));
    }

    #[test]
    fn stroke_keeps_the_path_and_style_as_coverage() {
        let path = Path::polyline([
            Point::new(0.0, 0.0),
            Point::new(10.0, 5.0),
            Point::new(20.0, 0.0),
        ]);
        let style = StrokeStyle::new(3.0).unwrap();
        let canvas = Canvas::draw(
            Color::rgb8(255, 255, 255),
            Coverage::stroke(path, style.clone()),
        );

        assert!(matches!(canvas.node(), CanvasNode::Draw(Draw {
                coverage: Coverage {
                    node: CoverageNode::Stroke { style: recorded, .. }
                },
                ..
            }) if *recorded == style));
    }

    #[test]
    fn rejects_invalid_stroke_widths_without_panicking() {
        assert_eq!(
            StrokeStyle::new(f32::NAN),
            Err(StrokeStyleError::NonFiniteWidth)
        );
        assert_eq!(StrokeStyle::new(-1.0), Err(StrokeStyleError::NegativeWidth));
        assert_eq!(StrokeStyle::new(0.0).unwrap().width(), 0.0);
    }

    #[test]
    fn stroke_style_defaults_to_butt_caps_and_miter_joins() {
        let style = StrokeStyle::new(3.0).unwrap();

        assert_eq!(style.cap(), LineCap::Butt);
        assert_eq!(style.join(), LineJoin::Miter);
    }

    #[test]
    fn stroke_cap_and_join_can_be_selected_independently() {
        let style = StrokeStyle::new(3.0)
            .unwrap()
            .with_cap(LineCap::Round)
            .with_join(LineJoin::Bevel);

        assert_eq!(style.width(), 3.0);
        assert_eq!(style.cap(), LineCap::Round);
        assert_eq!(style.join(), LineJoin::Bevel);
    }

    #[test]
    fn validates_dash_patterns_without_panicking() {
        assert_eq!(DashPattern::new([], 0.0), Err(DashPatternError::Empty));
        assert_eq!(
            DashPattern::new([2.0, f32::NAN], 0.0),
            Err(DashPatternError::NonFiniteInterval)
        );
        assert_eq!(
            DashPattern::new([2.0, 0.0], 0.0),
            Err(DashPatternError::NonPositiveInterval)
        );
        assert_eq!(
            DashPattern::new([2.0, 1.0], f32::INFINITY),
            Err(DashPatternError::NonFiniteOffset)
        );
    }

    #[test]
    fn stroke_style_can_add_and_remove_a_dash_pattern() {
        let pattern = DashPattern::new([4.0, 2.0], 1.0).unwrap();
        let style = StrokeStyle::new(3.0)
            .unwrap()
            .with_dash_pattern(pattern.clone());

        assert_eq!(style.dash_pattern(), Some(&pattern));
        assert!(style.without_dash_pattern().dash_pattern().is_none());
    }

    #[test]
    fn converts_straight_color_to_premultiplied_for_rendering() {
        let color = Color {
            red: 0.5,
            green: 0.25,
            blue: 1.0,
            alpha: 0.5,
        };
        assert_eq!(color.to_premultiplied_array(), [0.25, 0.125, 0.5, 0.5]);
    }

    #[test]
    fn converts_srgb_to_linear_for_rendering() {
        let linear = Color::rgb(0.5, 0.5, 0.5).unwrap().to_premultiplied_array();
        for component in &linear[..3] {
            assert!((*component - 0.214_041_14).abs() < 0.000_001);
        }
        assert_eq!(linear[3], 1.0);
        assert_eq!(
            Color::rgb8(128, 128, 128),
            Color::rgb(128.0 / 255.0, 128.0 / 255.0, 128.0 / 255.0).unwrap()
        );
    }

    #[test]
    fn rejects_invalid_color_components() {
        assert_eq!(
            Color::rgba(f32::NAN, 0.0, 0.0, 1.0),
            Err(ColorError::NonFiniteComponent)
        );
        assert_eq!(
            Color::rgba(1.1, 0.0, 0.0, 1.0),
            Err(ColorError::ComponentOutOfRange)
        );
        assert_eq!(
            Color::rgba(1.0, 0.0, 0.0, -0.1),
            Err(ColorError::ComponentOutOfRange)
        );
    }

    #[test]
    fn axis_aligned_points_map_to_normalized_view_corners() {
        let view =
            CanvasView::axis_aligned(Point::new(-100.0, 50.0), Point::new(300.0, 250.0)).unwrap();

        assert_eq!(
            view.canvas_to_view
                .try_transform_point(Point::new(-100.0, 50.0)),
            Some(Point::new(0.0, 0.0))
        );
        assert_eq!(
            view.canvas_to_view
                .try_transform_point(Point::new(300.0, 250.0)),
            Some(Point::new(1.0, 1.0))
        );
    }

    #[test]
    fn affine_points_map_to_the_normalized_view() {
        let origin = Point::new(10.0, 20.0);
        let p10 = origin + Vector2::new(20.0, 10.0);
        let p01 = origin + Vector2::new(-5.0, 15.0);
        let view = CanvasView::affine(origin, p10, p01).unwrap();

        assert_point_near(
            view.canvas_to_view.try_transform_point(origin),
            Point::new(0.0, 0.0),
        );
        assert_point_near(
            view.canvas_to_view.try_transform_point(p10),
            Point::new(1.0, 0.0),
        );
        assert_point_near(
            view.canvas_to_view.try_transform_point(p01),
            Point::new(0.0, 1.0),
        );
    }

    #[test]
    fn affine_rejects_collinear_points() {
        assert!(
            CanvasView::affine(
                Point::new(0.0, 0.0),
                Point::new(1.0, 2.0),
                Point::new(2.0, 4.0),
            )
            .is_none()
        );
    }

    #[test]
    fn projective_corners_map_to_normalized_view_corners() {
        let quad = [
            Point::new(-2.0, -1.0),
            Point::new(2.0, -1.0),
            Point::new(1.0, 1.0),
            Point::new(-1.0, 1.0),
        ];
        let [p00, p10, p11, p01] = quad;
        let view = CanvasView::projective(p00, p10, p11, p01).unwrap();
        let expected = [
            Point::new(0.0, 0.0),
            Point::new(1.0, 0.0),
            Point::new(1.0, 1.0),
            Point::new(0.0, 1.0),
        ];

        for (canvas, expected) in quad.into_iter().zip(expected) {
            let actual = view.canvas_to_view.try_transform_point(canvas).unwrap();
            assert!((actual.x - expected.x).abs() < 0.001);
            assert!((actual.y - expected.y).abs() < 0.001);
        }
    }

    #[test]
    fn view_can_give_canvas_coordinates_an_upward_y_direction() {
        let view =
            CanvasView::axis_aligned(Point::new(0.0, 100.0), Point::new(200.0, 0.0)).unwrap();

        assert_eq!(
            view.canvas_to_view
                .try_transform_point(Point::new(0.0, 0.0)),
            Some(Point::new(0.0, 1.0))
        );
        assert_eq!(
            view.canvas_to_view
                .try_transform_point(Point::new(0.0, 100.0)),
            Some(Point::new(0.0, 0.0))
        );
    }

    #[test]
    fn linear_gradient_keeps_ordered_stops_in_canvas_coordinates() {
        let stops = [
            GradientStop::new(0.0, Color::rgb8(255, 0, 0)).unwrap(),
            GradientStop::new(0.4, Color::rgb8(0, 255, 0)).unwrap(),
            GradientStop::new(1.0, Color::rgb8(0, 0, 255)).unwrap(),
        ];
        let gradient =
            LinearGradient::new(Point::new(10.0, 20.0), Point::new(110.0, 20.0), stops).unwrap();

        assert_eq!(gradient.start(), Point::new(10.0, 20.0));
        assert_eq!(gradient.end(), Point::new(110.0, 20.0));
        assert_eq!(gradient.stops(), stops);
    }

    #[test]
    fn along_path_gradient_keeps_a_path_and_reusable_color_ramp() {
        let path = Path::polyline([
            Point::new(0.0, 0.0),
            Point::new(10.0, 0.0),
            Point::new(10.0, 10.0),
        ]);
        let ramp = ColorRamp::new([
            GradientStop::new(0.0, Color::rgb8(255, 0, 0)).unwrap(),
            GradientStop::new(1.0, Color::rgb8(0, 0, 255)).unwrap(),
        ])
        .unwrap();
        let gradient = AlongPathGradient::new(path.clone(), ramp.clone());

        assert_eq!(gradient.path(), &path);
        assert_eq!(gradient.ramp(), &ramp);
        assert!(matches!(
            Source::from(gradient),
            Source::AlongPathGradient(_)
        ));
    }

    #[test]
    fn across_path_gradient_keeps_explicit_signed_distance_bounds() {
        let path = Path::polyline([Point::new(0.0, 0.0), Point::new(10.0, 0.0)]);
        let ramp = ColorRamp::new([
            GradientStop::new(0.0, Color::rgb8(255, 0, 0)).unwrap(),
            GradientStop::new(1.0, Color::rgb8(0, 0, 255)).unwrap(),
        ])
        .unwrap();
        let gradient = AcrossPathGradient::new(path.clone(), -2.0, 3.0, ramp.clone()).unwrap();

        assert_eq!(gradient.path(), &path);
        assert_eq!(gradient.from_distance(), -2.0);
        assert_eq!(gradient.to_distance(), 3.0);
        assert_eq!(gradient.ramp(), &ramp);
        assert!(matches!(
            Source::from(gradient),
            Source::AcrossPathGradient(_)
        ));
        assert_eq!(
            AcrossPathGradient::new(path.clone(), 1.0, 1.0, ramp.clone()),
            Err(AcrossPathGradientError::CoincidentDistances)
        );
        assert_eq!(
            AcrossPathGradient::new(path, f32::NAN, 1.0, ramp),
            Err(AcrossPathGradientError::NonFiniteDistance)
        );
    }

    #[test]
    fn color_ramp_rejects_too_few_or_unordered_stops() {
        let red = GradientStop::new(0.8, Color::rgb8(255, 0, 0)).unwrap();
        let blue = GradientStop::new(0.2, Color::rgb8(0, 0, 255)).unwrap();

        assert_eq!(ColorRamp::new([red]), Err(ColorRampError::TooFewStops));
        assert_eq!(
            ColorRamp::new([red, blue]),
            Err(ColorRampError::UnorderedStops)
        );
    }

    #[test]
    fn rejects_invalid_linear_gradient_structure_without_panicking() {
        let red = GradientStop::new(0.8, Color::rgb8(255, 0, 0)).unwrap();
        let blue = GradientStop::new(0.2, Color::rgb8(0, 0, 255)).unwrap();
        assert_eq!(
            GradientStop::new(f32::NAN, Color::rgb8(0, 0, 0)),
            Err(GradientStopError::NonFiniteOffset)
        );
        assert_eq!(
            GradientStop::new(1.1, Color::rgb8(0, 0, 0)),
            Err(GradientStopError::OffsetOutOfRange)
        );
        assert_eq!(
            LinearGradient::new(Point::new(0.0, 0.0), Point::new(0.0, 0.0), [red, blue]),
            Err(LinearGradientError::CoincidentPoints)
        );
        assert_eq!(
            LinearGradient::new(Point::new(0.0, 0.0), Point::new(1.0, 0.0), [red, blue]),
            Err(LinearGradientError::UnorderedStops)
        );
    }

    #[test]
    fn mesh_gradient_records_a_colored_triangle_patch() {
        let points = [
            GradientPoint::new(Point::new(0.0, 0.0), Color::rgb8(255, 0, 0)),
            GradientPoint::new(Point::new(4.0, 0.0), Color::rgb8(0, 255, 0)),
            GradientPoint::new(Point::new(0.0, 4.0), Color::rgb8(0, 0, 255)),
        ];
        let gradient = MeshGradient::from_triangle(points).unwrap();

        assert_eq!(gradient.triangles(), &[points]);
    }

    #[test]
    fn mesh_gradient_rejects_invalid_triangle_geometry() {
        let color = Color::rgb8(255, 255, 255);
        assert_eq!(
            MeshGradient::from_triangle([
                GradientPoint::new(Point::new(0.0, 0.0), color),
                GradientPoint::new(Point::new(1.0, 1.0), color),
                GradientPoint::new(Point::new(2.0, 2.0), color),
            ]),
            Err(MeshGradientError::DegenerateTriangle)
        );
        assert_eq!(
            MeshGradient::from_triangle([
                GradientPoint::new(Point::new(f32::NAN, 0.0), color),
                GradientPoint::new(Point::new(1.0, 0.0), color),
                GradientPoint::new(Point::new(0.0, 1.0), color),
            ]),
            Err(MeshGradientError::NonFinitePoint)
        );
    }

    #[test]
    fn radial_and_conic_gradients_keep_their_canvas_coordinates() {
        let stops = [
            GradientStop::new(0.0, Color::rgb8(255, 255, 255)).unwrap(),
            GradientStop::new(1.0, Color::rgb8(0, 0, 0)).unwrap(),
        ];
        let center = Point::new(20.0, 30.0);
        let radial = RadialGradient::new(center, 40.0, stops).unwrap();
        let conic = ConicGradient::new(center, 0.75, stops).unwrap();

        assert_eq!(radial.center(), center);
        assert_eq!(radial.radius(), 40.0);
        assert_eq!(radial.stops(), stops);
        assert_eq!(conic.center(), center);
        assert_eq!(conic.start_angle(), 0.75);
        assert_eq!(conic.stops(), stops);
    }

    #[test]
    fn radial_and_conic_gradients_reject_invalid_geometry() {
        let stops = [
            GradientStop::new(0.0, Color::rgb8(255, 255, 255)).unwrap(),
            GradientStop::new(1.0, Color::rgb8(0, 0, 0)).unwrap(),
        ];
        assert_eq!(
            RadialGradient::new(Point::new(0.0, 0.0), 0.0, stops),
            Err(RadialGradientError::NonPositiveRadius)
        );
        assert_eq!(
            RadialGradient::new(Point::new(0.0, 0.0), f32::NAN, stops),
            Err(RadialGradientError::NonFiniteRadius)
        );
        assert_eq!(
            ConicGradient::new(Point::new(0.0, 0.0), f32::INFINITY, stops),
            Err(ConicGradientError::NonFiniteStartAngle)
        );
    }
}
