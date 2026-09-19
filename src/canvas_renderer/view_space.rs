//! Conversion and precision decisions that depend on the rendered view.

use crate::{
    geometry::Point,
    path::{Path, PathSegment},
    tessellation::DEFAULT_TESSELLATION_TOLERANCE,
    transform::Projective2,
};

pub(crate) fn transformed_point_to_clip(point: Point, canvas_to_view: Projective2) -> [f32; 4] {
    let transformed = canvas_to_view.transform_homogeneous(point);
    [
        transformed.x * 2.0 - transformed.z,
        transformed.z - transformed.y * 2.0,
        0.0,
        transformed.z,
    ]
}

pub(crate) fn screen_space_tolerance(
    path: &Path,
    path_to_view: Projective2,
    viewport_width: f32,
    viewport_height: f32,
) -> f32 {
    const MIN_TOLERANCE: f32 = 1.0e-4;
    let mut maximum_scale = 0.0_f32;
    let points = path_reference_points(path);
    let points: &[Point] = if points.is_empty() {
        &[Point::new(0.0, 0.0)]
    } else {
        &points
    };

    for &point in points {
        let Some(scale) = local_pixel_scale(
            path_to_view,
            point,
            viewport_width.abs(),
            viewport_height.abs(),
        ) else {
            return MIN_TOLERANCE;
        };
        maximum_scale = maximum_scale.max(scale);
    }

    if maximum_scale == 0.0 {
        return DEFAULT_TESSELLATION_TOLERANCE;
    }
    let tolerance = DEFAULT_TESSELLATION_TOLERANCE / maximum_scale;
    if tolerance.is_finite() {
        tolerance.max(MIN_TOLERANCE)
    } else {
        MIN_TOLERANCE
    }
}

fn local_pixel_scale(
    transform: Projective2,
    point: Point,
    viewport_width: f32,
    viewport_height: f32,
) -> Option<f32> {
    let [[a, b, c], [d, e, f], [g, h, i]] = transform.rows();
    let numerator_x = a * point.x + b * point.y + c;
    let numerator_y = d * point.x + e * point.y + f;
    let denominator = g * point.x + h * point.y + i;
    if !denominator.is_finite() || denominator.abs() <= f32::EPSILON {
        return None;
    }
    let denominator_squared = denominator * denominator;
    let j00 = viewport_width * (a * denominator - g * numerator_x) / denominator_squared;
    let j01 = viewport_width * (b * denominator - h * numerator_x) / denominator_squared;
    let j10 = viewport_height * (d * denominator - g * numerator_y) / denominator_squared;
    let j11 = viewport_height * (e * denominator - h * numerator_y) / denominator_squared;
    if ![j00, j01, j10, j11].into_iter().all(f32::is_finite) {
        return None;
    }

    // Largest singular value of the 2x2 Jacobian: the greatest local stretch
    // from one Canvas unit to target pixels, independent of direction.
    let column0_squared = j00 * j00 + j10 * j10;
    let column1_squared = j01 * j01 + j11 * j11;
    let column_dot = j00 * j01 + j10 * j11;
    let discriminant =
        ((column0_squared - column1_squared).powi(2) + 4.0 * column_dot * column_dot).sqrt();
    let largest_eigenvalue = 0.5 * (column0_squared + column1_squared + discriminant);
    let scale = largest_eigenvalue.max(0.0).sqrt();
    scale.is_finite().then_some(scale)
}

fn path_reference_points(path: &Path) -> Vec<Point> {
    let mut points = Vec::new();
    for subpath in path.subpaths().iter().filter(|subpath| subpath.is_finite()) {
        let mut current = subpath.start();
        points.push(current);
        for segment in subpath.segments() {
            match *segment {
                PathSegment::LineTo(end) => points.push(end),
                PathSegment::QuadraticTo { control, end } => {
                    points.extend([control, end]);
                }
                PathSegment::CubicTo {
                    control1,
                    control2,
                    end,
                } => {
                    points.extend([control1, control2, end]);
                }
                PathSegment::EllipticalArcTo { radii, end, .. } => {
                    // SVG endpoint arcs may enlarge their supplied radii. This
                    // box deliberately overestimates the likely arc region so
                    // projective scale changes do not get missed completely.
                    let radius = radii
                        .x
                        .abs()
                        .max(radii.y.abs())
                        .max((end - current).length() * 0.5);
                    let center = Point::new((current.x + end.x) * 0.5, (current.y + end.y) * 0.5);
                    points.extend([
                        end,
                        Point::new(center.x - radius, center.y - radius),
                        Point::new(center.x + radius, center.y - radius),
                        Point::new(center.x + radius, center.y + radius),
                        Point::new(center.x - radius, center.y + radius),
                    ]);
                }
            }
            current = segment.end();
        }
    }
    points
}

#[cfg(test)]
mod tests {
    use crate::{
        canvas::CanvasView, geometry::Point, path::Path,
        tessellation::DEFAULT_TESSELLATION_TOLERANCE, transform::Projective2,
    };

    use super::{screen_space_tolerance, transformed_point_to_clip};

    fn point_to_clip(point: Point, view: CanvasView) -> [f32; 4] {
        transformed_point_to_clip(point, view.canvas_to_view)
    }

    #[test]
    fn path_tolerance_tracks_target_pixel_scale() {
        let path = Path::polyline([Point::new(0.0, 0.0), Point::new(1.0, 1.0)]);
        let identity = screen_space_tolerance(&path, Projective2::IDENTITY, 100.0, 100.0);
        let enlarged = screen_space_tolerance(
            &path,
            Projective2::from_rows([[10.0, 0.0, 0.0], [0.0, 10.0, 0.0], [0.0, 0.0, 1.0]]),
            100.0,
            100.0,
        );
        let reduced = screen_space_tolerance(
            &path,
            Projective2::from_rows([[0.1, 0.0, 0.0], [0.0, 0.1, 0.0], [0.0, 0.0, 1.0]]),
            100.0,
            100.0,
        );

        assert!((identity - DEFAULT_TESSELLATION_TOLERANCE / 100.0).abs() < 1.0e-6);
        assert!((enlarged - identity / 10.0).abs() < 1.0e-6);
        assert!((reduced - identity * 10.0).abs() < 1.0e-6);
    }

    #[test]
    fn converts_canvas_corners_to_clip_space() {
        let view =
            CanvasView::axis_aligned(Point::new(0.0, 0.0), Point::new(800.0, 600.0)).unwrap();
        assert_eq!(
            point_to_clip(Point::new(0.0, 0.0), view),
            [-1.0, 1.0, 0.0, 1.0]
        );
        assert_eq!(
            point_to_clip(Point::new(800.0, 600.0), view),
            [1.0, -1.0, 0.0, 1.0]
        );
    }

    #[test]
    fn applies_view_transform_before_projection() {
        let view =
            CanvasView::axis_aligned(Point::new(100.0, 50.0), Point::new(200.0, 100.0)).unwrap();
        assert_eq!(
            point_to_clip(Point::new(100.0, 50.0), view),
            [-1.0, 1.0, 0.0, 1.0]
        );
        assert_eq!(
            point_to_clip(Point::new(200.0, 100.0), view),
            [1.0, -1.0, 0.0, 1.0]
        );
    }

    #[test]
    fn preserves_projective_w_until_the_gpu_divide() {
        let quad = [
            Point::new(-2.0, -1.0),
            Point::new(2.0, -1.0),
            Point::new(1.0, 1.0),
            Point::new(-1.0, 1.0),
        ];
        let [p00, p10, p11, p01] = quad;
        let view = CanvasView::projective(p00, p10, p11, p01).unwrap();
        let expected_ndc = [[-1.0, 1.0], [1.0, 1.0], [1.0, -1.0], [-1.0, -1.0]];

        let clip_positions = quad.map(|point| point_to_clip(point, view));
        for (clip, expected) in clip_positions.into_iter().zip(expected_ndc) {
            assert!((clip[0] / clip[3] - expected[0]).abs() < 0.001);
            assert!((clip[1] / clip[3] - expected[1]).abs() < 0.001);
        }
        assert!(
            clip_positions
                .windows(2)
                .any(|pair| (pair[0][3] - pair[1][3]).abs() > 0.001)
        );
    }
}
