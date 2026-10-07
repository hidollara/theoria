use std::borrow::Borrow;

use crate::{
    geometry::Point,
    path::{
        DashPattern, FillRule, LineCap, LineJoin, Path, PathSegment, StrokeStyle, append_path,
        append_segment, build_lyon_path,
    },
};
use lyon_tessellation::{
    FillOptions, FillRule as LyonFillRule, FillTessellator, LineCap as LyonLineCap,
    LineJoin as LyonLineJoin, StrokeOptions, StrokeTessellator,
    geometry_builder::{BuffersBuilder, Positions, VertexBuffers},
    math::{Point as LyonPoint, point},
    path::{Path as LyonPath, PathEvent, builder::PathBuilder, iterator::PathIterator},
};

pub(crate) const DEFAULT_TESSELLATION_TOLERANCE: f32 = StrokeOptions::DEFAULT_TOLERANCE;

/// Backend-independent triangles produced from semantic vector geometry.
pub(crate) struct Mesh {
    pub vertices: Vec<Point>,
    pub indices: Vec<u32>,
}

#[cfg(test)]
pub(crate) fn tessellate_fill(path: &Path, rule: FillRule) -> Mesh {
    tessellate_fill_with_tolerance(path, rule, FillOptions::DEFAULT_TOLERANCE)
}

pub(crate) fn tessellate_fill_with_tolerance(path: &Path, rule: FillRule, tolerance: f32) -> Mesh {
    let mut buffers: VertexBuffers<LyonPoint, u32> = VertexBuffers::new();
    let result = {
        let mut output = BuffersBuilder::new(&mut buffers, Positions);
        let mut tessellator = FillTessellator::new();
        let options = FillOptions::default()
            .with_tolerance(tolerance)
            .with_fill_rule(match rule {
                FillRule::NonZero => LyonFillRule::NonZero,
                FillRule::EvenOdd => LyonFillRule::EvenOdd,
            });
        let mut builder = tessellator.builder(&options, &mut output).with_svg();
        for subpath in path.subpaths() {
            if !subpath.is_finite() {
                continue;
            }
            if subpath.segments().len() < 2
                && subpath
                    .segments()
                    .iter()
                    .all(|segment| matches!(segment, PathSegment::LineTo(_)))
            {
                continue;
            }
            let start = subpath.start();
            builder.move_to(point(start.x, start.y));
            for segment in subpath.segments() {
                append_segment(&mut builder, *segment);
            }
            // Fill semantics close open and closed Subpaths alike.
            builder.close();
        }
        builder.build()
    };

    if result.is_err() {
        return Mesh {
            vertices: Vec::new(),
            indices: Vec::new(),
        };
    }
    Mesh {
        vertices: buffers
            .vertices
            .into_iter()
            .map(|vertex| Point::new(vertex.x, vertex.y))
            .collect(),
        indices: buffers.indices,
    }
}

#[cfg(test)]
pub(crate) fn tessellate_stroke(path: &Path, style: impl Borrow<StrokeStyle>) -> Mesh {
    tessellate_stroke_with_tolerance(path, style, StrokeOptions::DEFAULT_TOLERANCE)
}

pub(crate) fn tessellate_stroke_with_tolerance(
    path: &Path,
    style: impl Borrow<StrokeStyle>,
    tolerance: f32,
) -> Mesh {
    let style = style.borrow();
    if style.width() == 0.0 {
        return Mesh {
            vertices: Vec::new(),
            indices: Vec::new(),
        };
    }
    let mut buffers: VertexBuffers<LyonPoint, u32> = VertexBuffers::new();
    let result = {
        let mut output = BuffersBuilder::new(&mut buffers, Positions);
        let mut tessellator = StrokeTessellator::new();
        let options = StrokeOptions::default()
            .with_tolerance(tolerance)
            .with_line_width(style.width())
            .with_line_cap(match style.cap() {
                LineCap::Butt => LyonLineCap::Butt,
                LineCap::Square => LyonLineCap::Square,
                LineCap::Round => LyonLineCap::Round,
            })
            .with_line_join(match style.join() {
                LineJoin::Miter => LyonLineJoin::Miter,
                LineJoin::Bevel => LyonLineJoin::Bevel,
                LineJoin::Round => LyonLineJoin::Round,
            });
        if let Some(pattern) = style.dash_pattern() {
            let source = build_lyon_path(path);
            let mut builder = tessellator.builder(&options, &mut output);
            append_dashed_path(&mut builder, &source, pattern, tolerance);
            builder.build()
        } else {
            let mut builder = tessellator.builder(&options, &mut output).with_svg();
            append_path(&mut builder, path);
            builder.build()
        }
    };

    if result.is_err() {
        return Mesh {
            vertices: Vec::new(),
            indices: Vec::new(),
        };
    }
    Mesh {
        vertices: buffers
            .vertices
            .into_iter()
            .map(|vertex| Point::new(vertex.x, vertex.y))
            .collect(),
        indices: buffers.indices,
    }
}

fn append_dashed_path(
    builder: &mut impl PathBuilder,
    path: &LyonPath,
    pattern: &DashPattern,
    tolerance: f32,
) {
    let mut cursor = DashCursor::new(pattern);
    let mut output_open = false;

    for event in path.iter().flattened(tolerance) {
        match event {
            PathEvent::Begin { .. } => {
                cursor = DashCursor::new(pattern);
                output_open = false;
            }
            PathEvent::Line { from, to } => {
                append_dashed_line(builder, from, to, &mut cursor, &mut output_open);
            }
            PathEvent::End { last, first, close } => {
                if close {
                    append_dashed_line(builder, last, first, &mut cursor, &mut output_open);
                }
                if output_open {
                    builder.end(false);
                    output_open = false;
                }
            }
            PathEvent::Quadratic { .. } | PathEvent::Cubic { .. } => {
                unreachable!("flattened path iteration only yields straight segments")
            }
        }
    }
}

fn append_dashed_line(
    builder: &mut impl PathBuilder,
    from: LyonPoint,
    to: LyonPoint,
    cursor: &mut DashCursor<'_>,
    output_open: &mut bool,
) {
    let delta = to - from;
    let length = delta.length();
    if length == 0.0 {
        return;
    }

    let mut consumed = 0.0;
    while consumed < length {
        let available = length - consumed;
        let reaches_boundary = cursor.remaining <= available;
        let step = cursor.remaining.min(available);
        let start = from + delta * (consumed / length);
        consumed += step;
        let end = if consumed >= length {
            to
        } else {
            from + delta * (consumed / length)
        };

        if cursor.painted {
            if !*output_open {
                builder.begin(start, &[]);
                *output_open = true;
            }
            builder.line_to(end, &[]);
        }

        if reaches_boundary {
            if cursor.painted && *output_open {
                builder.end(false);
                *output_open = false;
            }
            cursor.advance();
        } else {
            cursor.remaining -= step;
        }
    }
}

struct DashCursor<'a> {
    intervals: &'a [f32],
    index: usize,
    remaining: f32,
    painted: bool,
}

impl<'a> DashCursor<'a> {
    fn new(pattern: &'a DashPattern) -> Self {
        let intervals = pattern.intervals();
        let base_length: f32 = intervals.iter().sum();
        let cycle_length = if intervals.len().is_multiple_of(2) {
            base_length
        } else {
            base_length * 2.0
        };
        let mut cursor = Self {
            intervals,
            index: 0,
            remaining: intervals[0],
            painted: true,
        };
        let mut offset = pattern.offset().rem_euclid(cycle_length);
        while offset >= cursor.remaining {
            offset -= cursor.remaining;
            cursor.advance();
        }
        cursor.remaining -= offset;
        cursor
    }

    fn advance(&mut self) {
        self.index = (self.index + 1) % self.intervals.len();
        self.remaining = self.intervals[self.index];
        self.painted = !self.painted;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        geometry::Radii,
        path::{ArcSize, ArcSweep, Subpath},
    };

    fn square(min: f32, max: f32, reverse: bool) -> Subpath {
        let points = if reverse {
            [
                Point::new(min, min),
                Point::new(min, max),
                Point::new(max, max),
                Point::new(max, min),
            ]
        } else {
            [
                Point::new(min, min),
                Point::new(max, min),
                Point::new(max, max),
                Point::new(min, max),
            ]
        };
        Subpath::new(points[0])
            .line_to(points[1])
            .line_to(points[2])
            .line_to(points[3])
            .close()
    }

    fn covers(mesh: &Mesh, point: Point) -> bool {
        mesh.indices.chunks_exact(3).any(|triangle| {
            let a = mesh.vertices[triangle[0] as usize];
            let b = mesh.vertices[triangle[1] as usize];
            let c = mesh.vertices[triangle[2] as usize];
            let ab = signed_side(a, b, point);
            let bc = signed_side(b, c, point);
            let ca = signed_side(c, a, point);
            (ab >= 0.0 && bc >= 0.0 && ca >= 0.0) || (ab <= 0.0 && bc <= 0.0 && ca <= 0.0)
        })
    }

    fn signed_side(a: Point, b: Point, point: Point) -> f64 {
        (f64::from(b.x) - f64::from(a.x)) * (f64::from(point.y) - f64::from(a.y))
            - (f64::from(b.y) - f64::from(a.y)) * (f64::from(point.x) - f64::from(a.x))
    }

    #[test]
    fn tessellates_a_concave_path_into_an_indexed_mesh() {
        let path = Path::polygon([
            Point::new(0.0, 0.0),
            Point::new(4.0, 0.0),
            Point::new(4.0, 4.0),
            Point::new(2.0, 2.0),
            Point::new(0.0, 4.0),
        ]);

        let mesh = tessellate_fill(&path, FillRule::NonZero);
        assert_eq!(mesh.vertices.len(), 5);
        assert_eq!(mesh.indices.len(), 9);
    }

    #[test]
    fn accepts_either_winding_direction() {
        let path = Path::polygon([
            Point::new(0.0, 0.0),
            Point::new(0.0, 2.0),
            Point::new(2.0, 2.0),
            Point::new(2.0, 0.0),
        ]);

        assert_eq!(tessellate_fill(&path, FillRule::NonZero).indices.len(), 6);
    }

    #[test]
    fn filling_a_two_point_polyline_is_empty() {
        let path = Path::polyline([Point::new(0.0, 0.0), Point::new(2.0, 0.0)]);
        let mesh = tessellate_fill(&path, FillRule::NonZero);

        assert!(mesh.vertices.is_empty());
        assert!(mesh.indices.is_empty());
    }

    #[test]
    fn filling_implicitly_closes_an_open_subpath() {
        let path = Path::new().add_subpath(
            Subpath::new(Point::new(0.0, 0.0))
                .line_to(Point::new(2.0, 0.0))
                .line_to(Point::new(1.0, 2.0)),
        );

        assert_eq!(tessellate_fill(&path, FillRule::NonZero).indices.len(), 3);
    }

    #[test]
    fn fills_a_closed_region_bounded_by_a_cubic_bezier() {
        let path = Path::new().add_subpath(Subpath::new(Point::new(0.0, 0.0)).cubic_to(
            Point::new(0.0, 4.0),
            Point::new(4.0, 4.0),
            Point::new(4.0, 0.0),
        ));
        let mesh = tessellate_fill(&path, FillRule::NonZero);

        assert!(covers(&mesh, Point::new(2.0, 1.0)));
        assert!(mesh.vertices.len() > 3);
    }

    #[test]
    fn even_odd_makes_a_same_winding_inner_subpath_a_hole() {
        let path = Path::new()
            .add_subpath(square(0.0, 10.0, false))
            .add_subpath(square(3.0, 7.0, false));
        let mesh = tessellate_fill(&path, FillRule::EvenOdd);

        assert!(covers(&mesh, Point::new(1.0, 2.0)));
        assert!(!covers(&mesh, Point::new(5.0, 5.0)));
    }

    #[test]
    fn non_zero_uses_subpath_winding_to_determine_holes() {
        let hole = Path::new()
            .add_subpath(square(0.0, 10.0, false))
            .add_subpath(square(3.0, 7.0, true));
        let filled = Path::new()
            .add_subpath(square(0.0, 10.0, false))
            .add_subpath(square(3.0, 7.0, false));

        assert!(!covers(
            &tessellate_fill(&hole, FillRule::NonZero),
            Point::new(5.0, 5.0)
        ));
        assert!(covers(
            &tessellate_fill(&filled, FillRule::NonZero),
            Point::new(5.0, 5.0)
        ));
    }

    #[test]
    fn fills_self_intersecting_subpaths() {
        let path = Path::polygon([
            Point::new(0.0, 0.0),
            Point::new(4.0, 4.0),
            Point::new(0.0, 4.0),
            Point::new(4.0, 0.0),
        ]);
        let mesh = tessellate_fill(&path, FillRule::EvenOdd);

        assert!(covers(&mesh, Point::new(2.0, 3.5)));
        assert!(covers(&mesh, Point::new(2.0, 0.5)));
    }

    #[test]
    fn strokes_an_open_line_with_butt_caps() {
        let path = Path::polyline([Point::new(0.0, 0.0), Point::new(4.0, 0.0)]);
        let mesh = tessellate_stroke(&path, StrokeStyle::new(2.0).unwrap());

        assert_eq!(mesh.vertices.len(), 4);
        for expected in [
            Point::new(0.0, 1.0),
            Point::new(0.0, -1.0),
            Point::new(4.0, 1.0),
            Point::new(4.0, -1.0),
        ] {
            assert!(mesh.vertices.contains(&expected));
        }
        assert_eq!(mesh.indices.len(), 6);
    }

    #[test]
    fn dashes_alternate_painted_and_skipped_path_distances() {
        let path = Path::polyline([Point::new(0.0, 0.0), Point::new(10.0, 0.0)]);
        let style = StrokeStyle::new(1.0)
            .unwrap()
            .with_dash_pattern(DashPattern::new([2.0, 2.0], 0.0).unwrap());
        let mesh = tessellate_stroke(&path, style);

        assert!(covers(&mesh, Point::new(1.0, 0.0)));
        assert!(!covers(&mesh, Point::new(3.0, 0.0)));
        assert!(covers(&mesh, Point::new(5.0, 0.0)));
        assert!(!covers(&mesh, Point::new(7.0, 0.0)));
        assert!(covers(&mesh, Point::new(9.0, 0.0)));
        assert_eq!(mesh.indices.len(), 18);
    }

    #[test]
    fn dash_offset_moves_the_pattern_at_each_subpath_start() {
        let path = Path::polyline([Point::new(0.0, 0.0), Point::new(6.0, 0.0)]);
        let style = StrokeStyle::new(1.0)
            .unwrap()
            .with_dash_pattern(DashPattern::new([2.0, 2.0], 1.0).unwrap());
        let mesh = tessellate_stroke(&path, style);

        assert!(covers(&mesh, Point::new(0.5, 0.0)));
        assert!(!covers(&mesh, Point::new(2.0, 0.0)));
        assert!(covers(&mesh, Point::new(4.0, 0.0)));
    }

    #[test]
    fn odd_dash_patterns_repeat_with_their_phase_inverted() {
        let path = Path::polyline([Point::new(0.0, 0.0), Point::new(12.0, 0.0)]);
        let pattern = [2.0, 1.0, 1.0];
        let original = tessellate_stroke(
            &path,
            StrokeStyle::new(1.0)
                .unwrap()
                .with_dash_pattern(DashPattern::new(pattern, 0.0).unwrap()),
        );
        let one_full_cycle_later = tessellate_stroke(
            &path,
            StrokeStyle::new(1.0)
                .unwrap()
                .with_dash_pattern(DashPattern::new(pattern, 8.0).unwrap()),
        );

        assert_eq!(original.vertices, one_full_cycle_later.vertices);
        assert_eq!(original.indices, one_full_cycle_later.indices);
    }

    #[test]
    fn dashes_follow_curves_after_adaptive_flattening() {
        let path = Path::new().add_subpath(
            Subpath::new(Point::new(0.0, 0.0))
                .quadratic_to(Point::new(4.0, 8.0), Point::new(8.0, 0.0))
                .elliptical_arc_to(
                    Radii::new(4.0, 3.0),
                    0.2,
                    ArcSize::Minor,
                    ArcSweep::Positive,
                    Point::new(14.0, 2.0),
                ),
        );
        let mesh = tessellate_stroke(
            &path,
            StrokeStyle::new(0.5)
                .unwrap()
                .with_cap(LineCap::Round)
                .with_dash_pattern(DashPattern::new([1.5, 0.75], 0.0).unwrap()),
        );

        assert!(mesh.vertices.len() > 20);
        assert!(mesh.indices.len() > 30);
    }

    #[test]
    fn square_and_round_caps_extend_beyond_open_path_endpoints() {
        let path = Path::polyline([Point::new(0.0, 0.0), Point::new(4.0, 0.0)]);
        let square = tessellate_stroke(
            &path,
            StrokeStyle::new(2.0).unwrap().with_cap(LineCap::Square),
        );
        let round = tessellate_stroke(
            &path,
            StrokeStyle::new(2.0).unwrap().with_cap(LineCap::Round),
        );

        assert!(square.vertices.iter().any(|point| point.x == -1.0));
        assert!(square.vertices.iter().any(|point| point.x == 5.0));
        assert!(round.vertices.iter().any(|point| point.x < 0.0));
        assert!(round.vertices.iter().any(|point| point.x > 4.0));
        assert!(round.vertices.len() > square.vertices.len());
    }

    #[test]
    fn bevel_and_round_joins_produce_distinct_corner_geometry() {
        let path = Path::polyline([
            Point::new(0.0, 0.0),
            Point::new(4.0, 0.0),
            Point::new(4.0, 4.0),
        ]);
        let bevel = tessellate_stroke(
            &path,
            StrokeStyle::new(2.0).unwrap().with_join(LineJoin::Bevel),
        );
        let round = tessellate_stroke(
            &path,
            StrokeStyle::new(2.0).unwrap().with_join(LineJoin::Round),
        );

        assert_ne!(bevel.vertices, round.vertices);
        assert!(round.vertices.len() > bevel.vertices.len());
    }

    #[test]
    fn adaptively_flattens_quadratic_and_cubic_strokes() {
        let path = Path::new().add_subpath(
            Subpath::new(Point::new(0.0, 0.0))
                .quadratic_to(Point::new(2.0, 4.0), Point::new(4.0, 0.0))
                .cubic_to(
                    Point::new(6.0, -4.0),
                    Point::new(8.0, 4.0),
                    Point::new(10.0, 0.0),
                ),
        );
        let mesh = tessellate_stroke(&path, StrokeStyle::new(1.0).unwrap());

        assert!(mesh.vertices.len() > 8);
        assert!(mesh.indices.len() > 12);
    }

    #[test]
    fn explicit_tolerance_controls_curve_subdivision() {
        let path = Path::new().add_subpath(Subpath::new(Point::new(0.0, 0.0)).cubic_to(
            Point::new(0.0, 100.0),
            Point::new(100.0, -100.0),
            Point::new(100.0, 0.0),
        ));
        let style = StrokeStyle::new(1.0).unwrap();
        let coarse = tessellate_stroke_with_tolerance(&path, &style, 4.0);
        let fine = tessellate_stroke_with_tolerance(&path, &style, 0.01);

        assert!(fine.vertices.len() > coarse.vertices.len());
        assert!(fine.indices.len() > coarse.indices.len());
    }

    #[test]
    fn tessellates_elliptical_arcs_for_fill_and_stroke() {
        let ellipse = Path::new().add_subpath(
            Subpath::new(Point::new(-4.0, 0.0))
                .elliptical_arc_to(
                    Radii::new(4.0, 2.0),
                    0.0,
                    ArcSize::Minor,
                    ArcSweep::Positive,
                    Point::new(4.0, 0.0),
                )
                .elliptical_arc_to(
                    Radii::new(4.0, 2.0),
                    0.0,
                    ArcSize::Minor,
                    ArcSweep::Positive,
                    Point::new(-4.0, 0.0),
                )
                .close(),
        );

        let fill = tessellate_fill(&ellipse, FillRule::NonZero);
        let stroke = tessellate_stroke(&ellipse, StrokeStyle::new(0.5).unwrap());

        assert!(covers(&fill, Point::new(0.0, 0.0)));
        assert!(fill.vertices.len() > 6);
        assert!(stroke.vertices.len() > 12);
    }

    #[test]
    fn a_zero_arc_radius_degenerates_to_a_line() {
        let arc = Path::new().add_subpath(Subpath::new(Point::new(0.0, 0.0)).elliptical_arc_to(
            Radii::new(0.0, 3.0),
            0.0,
            ArcSize::Major,
            ArcSweep::Negative,
            Point::new(4.0, 0.0),
        ));
        let line = Path::polyline([Point::new(0.0, 0.0), Point::new(4.0, 0.0)]);

        let arc_mesh = tessellate_stroke(&arc, StrokeStyle::new(1.0).unwrap());
        let line_mesh = tessellate_stroke(&line, StrokeStyle::new(1.0).unwrap());
        assert_eq!(arc_mesh.vertices, line_mesh.vertices);
        assert_eq!(arc_mesh.indices, line_mesh.indices);
    }

    #[test]
    fn connects_every_edge_of_a_closed_stroke() {
        let path = Path::polygon([
            Point::new(0.0, 0.0),
            Point::new(4.0, 0.0),
            Point::new(2.0, 3.0),
        ]);
        let mesh = tessellate_stroke(&path, StrokeStyle::new(1.0).unwrap());

        assert!(covers(&mesh, Point::new(1.0, 1.5)));
        assert!(mesh.indices.len() >= 18);
    }

    #[test]
    fn strokes_multiple_subpaths_independently() {
        let path = Path::new()
            .add_subpath(Subpath::new(Point::new(0.0, 0.0)).line_to(Point::new(2.0, 0.0)))
            .add_subpath(Subpath::new(Point::new(0.0, 4.0)).line_to(Point::new(2.0, 4.0)));
        let mesh = tessellate_stroke(&path, StrokeStyle::new(1.0).unwrap());

        assert_eq!(mesh.vertices.len(), 8);
        assert_eq!(mesh.indices.len(), 12);
    }

    #[test]
    fn ignores_non_finite_subpaths_at_the_compilation_boundary() {
        let valid = Subpath::new(Point::new(0.0, 0.0)).line_to(Point::new(2.0, 0.0));
        let invalid = Subpath::new(Point::new(0.0, 4.0)).line_to(Point::new(f32::NAN, 4.0));
        let path = Path::new().add_subpath(valid).add_subpath(invalid);

        let mesh = tessellate_stroke(&path, StrokeStyle::new(1.0).unwrap());
        assert_eq!(mesh.vertices.len(), 4);
        assert_eq!(mesh.indices.len(), 6);
    }
}
