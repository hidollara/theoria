use lyon_path::{
    ArcFlags, Path as LyonPath, PathEvent,
    builder::SvgPathBuilder,
    iterator::PathIterator,
    math::{Angle, point, vector},
};

use crate::geometry::Point;

use super::{ArcSize, ArcSweep, Path, PathSegment};

/// Appends a segment after the builder's current point.
pub fn append_segment(builder: &mut impl SvgPathBuilder, segment: PathSegment) {
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

/// Appends finite subpaths, preserving curves and closure.
pub fn append_path(builder: &mut impl SvgPathBuilder, path: &Path) {
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

/// Converts finite subpaths to a Lyon path without flattening curves.
pub fn build_lyon_path(path: &Path) -> LyonPath {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::path::Subpath;

    #[test]
    fn preserves_subpath_boundaries_and_closure() {
        let a = Point::new(0.0, 0.0);
        let b = Point::new(1.0, 0.0);
        let c = Point::new(2.0, 0.0);
        let d = Point::new(3.0, 0.0);
        let path = Path::new()
            .add_subpath(Subpath::new(a).line_to(b).close())
            .add_subpath(Subpath::new(c).line_to(d));
        assert_eq!(
            flatten_subpaths(&path, 0.1),
            vec![vec![a, b, a], vec![c, d]]
        );
    }

    #[test]
    fn flattens_curves_with_the_requested_tolerance() {
        let start = Point::new(0.0, 0.0);
        let end = Point::new(10.0, 0.0);
        let path =
            Path::new().add_subpath(Subpath::new(start).quadratic_to(Point::new(5.0, 10.0), end));
        let coarse = flatten_subpaths(&path, 1.0);
        let fine = flatten_subpaths(&path, 0.01);
        assert_eq!(fine.len(), 1);
        assert_eq!(fine[0].first(), Some(&start));
        assert_eq!(fine[0].last(), Some(&end));
        assert!(fine[0].len() > coarse[0].len());
    }

    #[test]
    fn skips_non_finite_subpaths() {
        let start = Point::new(1.0, 2.0);
        let end = Point::new(3.0, 4.0);
        let path = Path::new()
            .add_subpath(Subpath::new(start).line_to(Point::new(f32::NAN, 0.0)))
            .add_subpath(Subpath::new(start).line_to(end));
        assert_eq!(flatten_subpaths(&path, 0.1), vec![vec![start, end]]);
        assert!(flatten_subpaths(&Path::new(), 0.1).is_empty());
    }
}
