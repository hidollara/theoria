use std::sync::Arc;

use crate::{geometry::Point, math::Vector2};

/// Selects the shorter or longer of the two elliptical arcs for a sweep direction.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ArcSize {
    #[default]
    Minor,
    Major,
}

/// Selects the direction in which an ellipse's parameter angle changes.
///
/// These names are coordinate-system-neutral: whether a positive sweep appears
/// clockwise depends on how Canvas coordinates are eventually viewed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ArcSweep {
    Negative,
    #[default]
    Positive,
}

/// One drawable segment following the current point of a [`Subpath`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PathSegment {
    LineTo(Point),
    QuadraticTo {
        control: Point,
        end: Point,
    },
    CubicTo {
        control1: Point,
        control2: Point,
        end: Point,
    },
    EllipticalArcTo {
        radii: Vector2,
        x_rotation: f32,
        size: ArcSize,
        sweep: ArcSweep,
        end: Point,
    },
}

impl PathSegment {
    pub const fn end(self) -> Point {
        match self {
            Self::LineTo(point) => point,
            Self::QuadraticTo { end, .. }
            | Self::CubicTo { end, .. }
            | Self::EllipticalArcTo { end, .. } => end,
        }
    }

    fn is_finite(self) -> bool {
        match self {
            Self::LineTo(point) => point.is_finite(),
            Self::QuadraticTo { control, end } => control.is_finite() && end.is_finite(),
            Self::CubicTo {
                control1,
                control2,
                end,
            } => control1.is_finite() && control2.is_finite() && end.is_finite(),
            Self::EllipticalArcTo {
                radii,
                x_rotation,
                end,
                ..
            } => {
                radii.x.is_finite()
                    && radii.y.is_finite()
                    && x_rotation.is_finite()
                    && end.is_finite()
            }
        }
    }
}

/// One continuous sequence of segments beginning at an explicit point.
///
/// Closing is a property of the complete Subpath rather than a segment in its
/// ordered data. It connects the final point back to `start` when evaluated.
#[derive(Clone, Debug, PartialEq)]
pub struct Subpath {
    start: Point,
    segments: Arc<Vec<PathSegment>>,
    closed: bool,
}

impl Subpath {
    /// Creates an open Subpath beginning at `start`.
    pub fn new(start: Point) -> Self {
        Self {
            start,
            segments: Arc::new(Vec::new()),
            closed: false,
        }
    }

    /// Appends a straight segment.
    pub fn line_to(mut self, point: Point) -> Self {
        Arc::make_mut(&mut self.segments).push(PathSegment::LineTo(point));
        self
    }

    /// Appends a quadratic Bezier segment with one control point.
    pub fn quadratic_to(mut self, control: Point, end: Point) -> Self {
        Arc::make_mut(&mut self.segments).push(PathSegment::QuadraticTo { control, end });
        self
    }

    /// Appends a cubic Bezier segment with two control points.
    pub fn cubic_to(mut self, control1: Point, control2: Point, end: Point) -> Self {
        Arc::make_mut(&mut self.segments).push(PathSegment::CubicTo {
            control1,
            control2,
            end,
        });
        self
    }

    /// Appends an elliptical arc using endpoint notation.
    ///
    /// `radii` are the ellipse's two radii, `x_rotation` rotates its local x
    /// axis in radians, and `end` is the destination point. Radius signs have
    /// no geometric meaning and are normalized by renderers. A zero radius
    /// degenerates to a straight segment.
    pub fn elliptical_arc_to(
        mut self,
        radii: Vector2,
        x_rotation: f32,
        size: ArcSize,
        sweep: ArcSweep,
        end: Point,
    ) -> Self {
        Arc::make_mut(&mut self.segments).push(PathSegment::EllipticalArcTo {
            radii,
            x_rotation,
            size,
            sweep,
            end,
        });
        self
    }

    /// Marks this Subpath as closed. Repeated calls have no further effect.
    pub fn close(mut self) -> Self {
        self.closed = true;
        self
    }

    /// Marks this Subpath as open again.
    pub fn reopen(mut self) -> Self {
        self.closed = false;
        self
    }

    pub const fn start(&self) -> Point {
        self.start
    }

    pub fn segments(&self) -> &[PathSegment] {
        &self.segments
    }

    pub const fn is_closed(&self) -> bool {
        self.closed
    }

    /// Reports whether every coordinate can safely enter geometric evaluation.
    pub fn is_finite(&self) -> bool {
        self.start.is_finite() && self.segments.iter().all(|segment| segment.is_finite())
    }
}

/// A collection of independent Subpaths in Canvas coordinates.
///
/// Path construction is structural and infallible. Numeric validity is
/// inspected when a renderer compiles its Subpaths into backend geometry.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Path {
    subpaths: Arc<Vec<Subpath>>,
}

impl Path {
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends a Subpath without changing its geometry or closure.
    pub fn add_subpath(mut self, subpath: Subpath) -> Self {
        Arc::make_mut(&mut self.subpaths).push(subpath);
        self
    }

    /// Creates a Path from Subpaths in their evaluation order.
    pub fn from_subpaths(subpaths: impl IntoIterator<Item = Subpath>) -> Self {
        Self {
            subpaths: Arc::new(subpaths.into_iter().collect()),
        }
    }

    /// Creates a Path containing one closed, straight-edged Subpath.
    ///
    /// An empty point sequence creates an empty Path. Degenerate and
    /// self-intersecting geometry remains valid semantic Path data.
    pub fn polygon(points: impl IntoIterator<Item = Point>) -> Self {
        Self::from_points(points, true)
    }

    /// Creates a Path containing one open, straight-edged Subpath.
    pub fn polyline(points: impl IntoIterator<Item = Point>) -> Self {
        Self::from_points(points, false)
    }

    pub fn subpaths(&self) -> &[Subpath] {
        &self.subpaths
    }

    pub fn is_empty(&self) -> bool {
        self.subpaths.is_empty()
    }

    /// Reports whether every Subpath contains only finite coordinates.
    pub fn is_finite(&self) -> bool {
        self.subpaths.iter().all(Subpath::is_finite)
    }

    fn from_points(points: impl IntoIterator<Item = Point>, closed: bool) -> Self {
        let mut points = points.into_iter();
        let Some(start) = points.next() else {
            return Self::new();
        };
        let mut subpath = Subpath::new(start);
        for point in points {
            subpath = subpath.line_to(point);
        }
        if closed {
            subpath = subpath.close();
        }
        Self::new().add_subpath(subpath)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_subpath_starts_open_and_can_change_closure() {
        let subpath = Subpath::new(Point::new(0.0, 0.0))
            .line_to(Point::new(1.0, 0.0))
            .close()
            .close();
        assert!(subpath.is_closed());

        let subpath = subpath.reopen();
        assert!(!subpath.is_closed());
    }

    #[test]
    fn path_only_collects_independent_subpaths() {
        let first = Subpath::new(Point::new(0.0, 0.0))
            .line_to(Point::new(2.0, 0.0))
            .close();
        let second = Subpath::new(Point::new(4.0, 0.0)).line_to(Point::new(6.0, 0.0));
        let path = Path::new().add_subpath(first).add_subpath(second);

        assert_eq!(path.subpaths().len(), 2);
        assert!(path.subpaths()[0].is_closed());
        assert!(!path.subpaths()[1].is_closed());
    }

    #[test]
    fn polygon_and_polyline_are_subpath_templates() {
        let polygon = Path::polygon([
            Point::new(0.0, 0.0),
            Point::new(2.0, 0.0),
            Point::new(2.0, 2.0),
        ]);
        let polyline = Path::polyline([
            Point::new(0.0, 0.0),
            Point::new(2.0, 1.0),
            Point::new(4.0, 0.0),
        ]);

        assert!(polygon.subpaths()[0].is_closed());
        assert!(!polyline.subpaths()[0].is_closed());
    }

    #[test]
    fn records_quadratic_and_cubic_segments_without_flattening_them() {
        let quadratic_control = Point::new(1.0, 2.0);
        let quadratic_end = Point::new(2.0, 0.0);
        let cubic_control1 = Point::new(3.0, -2.0);
        let cubic_control2 = Point::new(4.0, 2.0);
        let cubic_end = Point::new(5.0, 0.0);
        let subpath = Subpath::new(Point::new(0.0, 0.0))
            .quadratic_to(quadratic_control, quadratic_end)
            .cubic_to(cubic_control1, cubic_control2, cubic_end);

        assert_eq!(
            subpath.segments(),
            [
                PathSegment::QuadraticTo {
                    control: quadratic_control,
                    end: quadratic_end,
                },
                PathSegment::CubicTo {
                    control1: cubic_control1,
                    control2: cubic_control2,
                    end: cubic_end,
                },
            ]
        );
    }

    #[test]
    fn records_elliptical_arc_semantics_without_flattening_them() {
        let radii = Vector2::new(4.0, 2.0);
        let end = Point::new(8.0, 1.0);
        let subpath = Subpath::new(Point::new(0.0, 0.0)).elliptical_arc_to(
            radii,
            0.25,
            ArcSize::Major,
            ArcSweep::Negative,
            end,
        );

        assert_eq!(
            subpath.segments(),
            [PathSegment::EllipticalArcTo {
                radii,
                x_rotation: 0.25,
                size: ArcSize::Major,
                sweep: ArcSweep::Negative,
                end,
            }]
        );
        assert_eq!(subpath.segments()[0].end(), end);
    }

    #[test]
    fn empty_templates_create_an_empty_path() {
        assert!(Path::polygon([]).is_empty());
        assert!(Path::polyline([]).is_empty());
    }

    #[test]
    fn self_intersections_are_semantic_path_data() {
        let path = Path::polygon([
            Point::new(0.0, 0.0),
            Point::new(2.0, 2.0),
            Point::new(0.0, 2.0),
            Point::new(2.0, 0.0),
        ]);

        assert_eq!(path.subpaths().len(), 1);
    }

    #[test]
    fn non_finite_coordinates_are_recorded_without_failing_construction() {
        let path = Path::new().add_subpath(
            Subpath::new(Point::new(0.0, 0.0)).line_to(Point::new(f32::NAN, f32::INFINITY)),
        );

        assert!(!path.is_finite());
    }

    #[test]
    fn non_finite_bezier_controls_are_reported_without_failing_construction() {
        let path = Path::new().add_subpath(
            Subpath::new(Point::new(0.0, 0.0))
                .quadratic_to(Point::new(f32::NAN, 1.0), Point::new(2.0, 0.0)),
        );

        assert!(!path.is_finite());
    }

    #[test]
    fn non_finite_arc_parameters_are_reported_without_failing_construction() {
        let path = Path::new().add_subpath(Subpath::new(Point::new(0.0, 0.0)).elliptical_arc_to(
            Vector2::new(2.0, f32::INFINITY),
            f32::NAN,
            ArcSize::Minor,
            ArcSweep::Positive,
            Point::new(4.0, 0.0),
        ));

        assert!(!path.is_finite());
    }
}
