use lyon_tessellation::{
    math::{Angle, point, vector},
    path::{
        ArcFlags, Path as LyonPath, PathEvent, builder::SvgPathBuilder, iterator::PathIterator,
    },
};

use theoria_canvas::geometry::Point;

use super::{ArcSize, ArcSweep, Path, PathSegment};

pub(crate) fn append_segment(builder: &mut impl SvgPathBuilder, segment: PathSegment) {
    match segment {
        PathSegment::LineTo(end) => builder.line_to(point(end.x, end.y)),
        PathSegment::QuadraticTo { control, end } => {
            builder.quadratic_bezier_to(point(control.x, control.y), point(end.x, end.y))
        }
        PathSegment::CubicTo {
            control1,
            control2,
            end,
        } => builder.cubic_bezier_to(
            point(control1.x, control1.y),
            point(control2.x, control2.y),
            point(end.x, end.y),
        ),
        PathSegment::EllipticalArcTo {
            radii,
            x_rotation,
            size,
            sweep,
            end,
        } => builder.arc_to(
            vector(radii.x.abs(), radii.y.abs()),
            Angle::radians(x_rotation),
            ArcFlags {
                large_arc: size == ArcSize::Major,
                sweep: sweep == ArcSweep::Positive,
            },
            point(end.x, end.y),
        ),
    }
}

pub(crate) fn append_path(builder: &mut impl SvgPathBuilder, path: &Path) {
    for subpath in path.subpaths() {
        if !subpath.is_finite() {
            continue;
        }
        let start = subpath.start();
        builder.move_to(point(start.x, start.y));
        for segment in subpath.segments() {
            append_segment(builder, *segment);
        }
        if subpath.is_closed() {
            builder.close();
        }
    }
}

pub(crate) fn build_lyon_path(path: &Path) -> LyonPath {
    let mut builder = LyonPath::builder().with_svg();
    append_path(&mut builder, path);
    builder.build()
}

/// Flattens each valid Subpath independently while preserving closure.
pub fn flatten_subpaths(path: &Path, tolerance: f32) -> Vec<Vec<Point>> {
    let source = build_lyon_path(path);
    let mut output = Vec::new();
    let mut current = Vec::new();
    for event in source.iter().flattened(tolerance) {
        match event {
            PathEvent::Begin { at } => {
                current = vec![Point::new(at.x, at.y)];
            }
            PathEvent::Line { to, .. } => current.push(Point::new(to.x, to.y)),
            PathEvent::End { first, close, .. } => {
                if close && current.last().copied() != Some(Point::new(first.x, first.y)) {
                    current.push(Point::new(first.x, first.y));
                }
                output.push(std::mem::take(&mut current));
            }
            PathEvent::Quadratic { .. } | PathEvent::Cubic { .. } => {
                unreachable!("flattened path iteration only yields straight segments")
            }
        }
    }
    output
}
