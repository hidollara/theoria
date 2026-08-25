use crate::{
    canvas::{ColorField, GradientPoint, GradientStop, LinearGradient, MeshGradient},
    geometry::Point,
    path::{Path, PathSegment},
    tessellation::{DEFAULT_TESSELLATION_TOLERANCE, flatten_subpaths},
    transform::Projective2,
};

#[cfg(test)]
use crate::canvas::CanvasView;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct PaintedTriangle {
    pub(super) points: [Point; 3],
    pub(super) colors: [[f32; 4]; 3],
}

#[derive(Clone, Copy)]
struct GradientVertex {
    point: Point,
    pub(super) offset: f32,
}

#[cfg(test)]
pub(super) fn paint_triangle(points: [Point; 3], source: &ColorField) -> Vec<PaintedTriangle> {
    let path_sampler = match source {
        ColorField::AlongPathGradient(gradient) => Some(PathSampler::new(
            gradient.path(),
            DEFAULT_TESSELLATION_TOLERANCE,
        )),
        ColorField::AcrossPathGradient(gradient) => Some(PathSampler::new(
            gradient.path(),
            DEFAULT_TESSELLATION_TOLERANCE,
        )),
        _ => None,
    };
    paint_triangle_prepared(points, source, path_sampler.as_ref())
}

pub(super) fn paint_triangle_prepared(
    points: [Point; 3],
    source: &ColorField,
    path_sampler: Option<&PathSampler>,
) -> Vec<PaintedTriangle> {
    match source {
        ColorField::Solid(color) => vec![PaintedTriangle {
            points,
            colors: [color.to_premultiplied_array(); 3],
        }],
        ColorField::LinearGradient(gradient) => paint_linear_gradient(points, gradient),
        ColorField::RadialGradient(gradient) => paint_sampled_gradient(points, |point| {
            let offset = (point - gradient.center()).length() / gradient.radius();
            evaluate_gradient_stops(gradient.stops(), offset)
        }),
        ColorField::ConicGradient(gradient) => paint_sampled_gradient(points, |point| {
            let delta = point - gradient.center();
            let angle = delta.y.atan2(delta.x);
            let offset = (angle - gradient.start_angle()).rem_euclid(std::f32::consts::TAU)
                / std::f32::consts::TAU;
            evaluate_gradient_stops(gradient.stops(), offset)
        }),
        ColorField::AlongPathGradient(gradient) => {
            let sampler = path_sampler.expect("along-Path sources prepare their Path once");
            paint_sampled_gradient(points, |point| {
                let offset = sampler.nearest_offset(point).unwrap_or(0.0);
                evaluate_gradient_stops(gradient.ramp().stops(), offset)
            })
        }
        ColorField::AcrossPathGradient(gradient) => {
            let sampler = path_sampler.expect("across-Path sources prepare their Path once");
            paint_sampled_gradient(points, |point| {
                let distance = sampler
                    .nearest_sample(point)
                    .map_or(gradient.from_distance(), |sample| sample.signed_distance);
                let offset = (distance - gradient.from_distance())
                    / (gradient.to_distance() - gradient.from_distance());
                evaluate_gradient_stops(gradient.ramp().stops(), offset)
            })
        }
        ColorField::MeshGradient(gradient) => paint_mesh_gradient(points, gradient),
        ColorField::Image(_) => {
            unreachable!("image color fields use the sampled-image pipeline")
        }
        ColorField::Canvas(_) => {
            unreachable!("Canvas color fields are evaluated before mesh drawing")
        }
    }
}

pub(super) struct PathSampler {
    subpaths: Vec<MeasuredSubpath>,
}

struct MeasuredSubpath {
    segments: Vec<MeasuredSegment>,
    length: f32,
}

struct MeasuredSegment {
    from: Point,
    to: Point,
    start: f32,
    length: f32,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct PathSample {
    offset: f32,
    pub(super) signed_distance: f32,
}

impl PathSampler {
    pub(super) fn new(path: &Path, tolerance: f32) -> Self {
        let subpaths = flatten_subpaths(path, tolerance)
            .into_iter()
            .map(|points| {
                let mut length = 0.0;
                let segments = points
                    .windows(2)
                    .filter_map(|pair| {
                        let from = pair[0];
                        let to = pair[1];
                        let segment_length = (to - from).length();
                        if segment_length == 0.0 || !segment_length.is_finite() {
                            return None;
                        }
                        let segment = MeasuredSegment {
                            from,
                            to,
                            start: length,
                            length: segment_length,
                        };
                        length += segment_length;
                        Some(segment)
                    })
                    .collect();
                MeasuredSubpath { segments, length }
            })
            .filter(|subpath| subpath.length > 0.0)
            .collect();
        Self { subpaths }
    }

    pub(super) fn nearest_offset(&self, point: Point) -> Option<f32> {
        self.nearest_sample(point).map(|sample| sample.offset)
    }

    pub(super) fn nearest_sample(&self, point: Point) -> Option<PathSample> {
        let mut nearest: Option<(f32, PathSample)> = None;
        for subpath in &self.subpaths {
            for segment in &subpath.segments {
                let direction = segment.to - segment.from;
                let amount = ((point - segment.from).dot(direction)
                    / (segment.length * segment.length))
                    .clamp(0.0, 1.0);
                let closest = segment.from + direction * amount;
                let displacement = point - closest;
                let distance_squared = displacement.dot(displacement);
                let offset = (segment.start + segment.length * amount) / subpath.length;
                let signed_distance =
                    (direction.x * displacement.y - direction.y * displacement.x) / segment.length;
                let sample = PathSample {
                    offset,
                    signed_distance,
                };
                if nearest.is_none_or(|(best_distance, _)| distance_squared < best_distance) {
                    nearest = Some((distance_squared, sample));
                }
            }
        }
        nearest.map(|(_, sample)| sample)
    }
}

fn paint_linear_gradient(points: [Point; 3], gradient: &LinearGradient) -> Vec<PaintedTriangle> {
    let axis = gradient.end() - gradient.start();
    let length_squared = axis.dot(axis);
    let original: Vec<_> = points
        .map(|point| GradientVertex {
            point,
            offset: (point - gradient.start()).dot(axis) / length_squared,
        })
        .into();
    let stops = gradient.stops();
    let mut output = Vec::new();
    let mut remaining = original;

    let first = stops[0];
    let before = clip_gradient_polygon(&remaining, first.offset(), true);
    append_gradient_polygon(
        &mut output,
        &before,
        first.offset(),
        first.offset(),
        first.color().to_premultiplied_array(),
        first.color().to_premultiplied_array(),
    );
    remaining = clip_gradient_polygon(&remaining, first.offset(), false);

    for pair in stops.windows(2) {
        let [from, to] = [pair[0], pair[1]];
        let band = clip_gradient_polygon(&remaining, to.offset(), true);
        append_gradient_polygon(
            &mut output,
            &band,
            from.offset(),
            to.offset(),
            from.color().to_premultiplied_array(),
            to.color().to_premultiplied_array(),
        );
        remaining = clip_gradient_polygon(&remaining, to.offset(), false);
    }

    let last = *stops.last().unwrap();
    append_gradient_polygon(
        &mut output,
        &remaining,
        last.offset(),
        last.offset(),
        last.color().to_premultiplied_array(),
        last.color().to_premultiplied_array(),
    );
    output
}

fn clip_gradient_polygon(
    polygon: &[GradientVertex],
    boundary: f32,
    keep_below: bool,
) -> Vec<GradientVertex> {
    let Some(mut previous) = polygon.last().copied() else {
        return Vec::new();
    };
    let mut previous_inside = if keep_below {
        previous.offset <= boundary
    } else {
        previous.offset >= boundary
    };
    let mut output = Vec::with_capacity(polygon.len() + 1);

    for &current in polygon {
        let current_inside = if keep_below {
            current.offset <= boundary
        } else {
            current.offset >= boundary
        };
        if previous_inside != current_inside {
            let amount = (boundary - previous.offset) / (current.offset - previous.offset);
            output.push(GradientVertex {
                point: Point::new(
                    previous.point.x + (current.point.x - previous.point.x) * amount,
                    previous.point.y + (current.point.y - previous.point.y) * amount,
                ),
                offset: boundary,
            });
        }
        if current_inside {
            output.push(current);
        }
        previous = current;
        previous_inside = current_inside;
    }
    output
}

fn append_gradient_polygon(
    output: &mut Vec<PaintedTriangle>,
    polygon: &[GradientVertex],
    start_offset: f32,
    end_offset: f32,
    start_color: [f32; 4],
    end_color: [f32; 4],
) {
    if polygon.len() < 3 {
        return;
    }
    for index in 1..polygon.len() - 1 {
        let vertices = [polygon[0], polygon[index], polygon[index + 1]];
        let twice_area = (vertices[1].point - vertices[0].point).x
            * (vertices[2].point - vertices[0].point).y
            - (vertices[1].point - vertices[0].point).y * (vertices[2].point - vertices[0].point).x;
        if twice_area == 0.0 {
            continue;
        }
        output.push(PaintedTriangle {
            points: vertices.map(|vertex| vertex.point),
            colors: vertices.map(|vertex| {
                let amount = if start_offset == end_offset {
                    0.0
                } else {
                    ((vertex.offset - start_offset) / (end_offset - start_offset)).clamp(0.0, 1.0)
                };
                std::array::from_fn(|component| {
                    start_color[component]
                        + (end_color[component] - start_color[component]) * amount
                })
            }),
        });
    }
}

fn evaluate_gradient_stops(stops: &[GradientStop], offset: f32) -> [f32; 4] {
    if offset <= stops[0].offset() {
        return stops[0].color().to_premultiplied_array();
    }
    for pair in stops.windows(2) {
        let [from, to] = [pair[0], pair[1]];
        if offset < to.offset() {
            let amount = (offset - from.offset()) / (to.offset() - from.offset());
            let from_color = from.color().to_premultiplied_array();
            let to_color = to.color().to_premultiplied_array();
            return std::array::from_fn(|component| {
                from_color[component] + (to_color[component] - from_color[component]) * amount
            });
        }
    }
    stops.last().unwrap().color().to_premultiplied_array()
}

fn paint_sampled_gradient(
    points: [Point; 3],
    sample: impl Fn(Point) -> [f32; 4] + Copy,
) -> Vec<PaintedTriangle> {
    let mut output = Vec::new();
    subdivide_sampled_gradient(points, sample, 0, &mut output);
    output
}

fn subdivide_sampled_gradient(
    points: [Point; 3],
    sample: impl Fn(Point) -> [f32; 4] + Copy,
    depth: u32,
    output: &mut Vec<PaintedTriangle>,
) {
    const MIN_DEPTH: u32 = 2;
    const MAX_DEPTH: u32 = 7;
    const COLOR_TOLERANCE: f32 = 1.0 / 255.0;

    let colors = points.map(sample);
    let midpoints = [
        midpoint(points[0], points[1]),
        midpoint(points[1], points[2]),
        midpoint(points[2], points[0]),
    ];
    let center = Point::new(
        (points[0].x + points[1].x + points[2].x) / 3.0,
        (points[0].y + points[1].y + points[2].y) / 3.0,
    );
    let mut error: f32 = 0.0;
    for edge in 0..3 {
        let actual = sample(midpoints[edge]);
        let expected = std::array::from_fn(|component| {
            (colors[edge][component] + colors[(edge + 1) % 3][component]) * 0.5
        });
        error = error.max(color_difference(actual, expected));
    }
    let actual_center = sample(center);
    let expected_center = std::array::from_fn(|component| {
        (colors[0][component] + colors[1][component] + colors[2][component]) / 3.0
    });
    error = error.max(color_difference(actual_center, expected_center));

    if depth >= MAX_DEPTH || (depth >= MIN_DEPTH && error <= COLOR_TOLERANCE) {
        output.push(PaintedTriangle { points, colors });
        return;
    }

    let [m01, m12, m20] = midpoints;
    for child in [
        [points[0], m01, m20],
        [m01, points[1], m12],
        [m20, m12, points[2]],
        [m01, m12, m20],
    ] {
        subdivide_sampled_gradient(child, sample, depth + 1, output);
    }
}

fn midpoint(first: Point, second: Point) -> Point {
    Point::new((first.x + second.x) * 0.5, (first.y + second.y) * 0.5)
}

fn color_difference(first: [f32; 4], second: [f32; 4]) -> f32 {
    first
        .into_iter()
        .zip(second)
        .map(|(first, second)| (first - second).abs())
        .fold(0.0, f32::max)
}

fn paint_mesh_gradient(points: [Point; 3], gradient: &MeshGradient) -> Vec<PaintedTriangle> {
    let mut output = Vec::new();
    for patch in gradient.triangles() {
        let patch_points = patch.map(GradientPoint::position);
        let orientation = signed_area(patch_points);
        let mut polygon = points.to_vec();
        for edge in 0..3 {
            polygon = clip_polygon_to_edge(
                &polygon,
                patch_points[edge],
                patch_points[(edge + 1) % 3],
                orientation,
            );
        }
        if polygon.len() < 3 {
            continue;
        }
        for index in 1..polygon.len() - 1 {
            let triangle_points = [polygon[0], polygon[index], polygon[index + 1]];
            if signed_area(triangle_points) == 0.0 {
                continue;
            }
            output.push(PaintedTriangle {
                points: triangle_points,
                colors: triangle_points.map(|point| mesh_color_at(point, *patch)),
            });
        }
    }
    output
}

fn clip_polygon_to_edge(
    polygon: &[Point],
    edge_start: Point,
    edge_end: Point,
    orientation: f32,
) -> Vec<Point> {
    let Some(mut previous) = polygon.last().copied() else {
        return Vec::new();
    };
    let mut previous_side = edge_side(edge_start, edge_end, previous);
    let mut previous_inside = previous_side * orientation >= 0.0;
    let mut output = Vec::with_capacity(polygon.len() + 1);
    for &current in polygon {
        let current_side = edge_side(edge_start, edge_end, current);
        let current_inside = current_side * orientation >= 0.0;
        if previous_inside != current_inside {
            let amount = previous_side / (previous_side - current_side);
            output.push(Point::new(
                previous.x + (current.x - previous.x) * amount,
                previous.y + (current.y - previous.y) * amount,
            ));
        }
        if current_inside {
            output.push(current);
        }
        previous = current;
        previous_side = current_side;
        previous_inside = current_inside;
    }
    output
}

fn mesh_color_at(point: Point, patch: [GradientPoint; 3]) -> [f32; 4] {
    let positions = patch.map(GradientPoint::position);
    let denominator = signed_area(positions);
    let weights = [
        edge_side(positions[1], positions[2], point) / denominator,
        edge_side(positions[2], positions[0], point) / denominator,
        edge_side(positions[0], positions[1], point) / denominator,
    ];
    let colors = patch.map(|vertex| vertex.color().to_premultiplied_array());
    std::array::from_fn(|component| {
        colors[0][component] * weights[0]
            + colors[1][component] * weights[1]
            + colors[2][component] * weights[2]
    })
}

fn signed_area(points: [Point; 3]) -> f32 {
    edge_side(points[0], points[1], points[2])
}

fn edge_side(start: Point, end: Point, point: Point) -> f32 {
    (end.x - start.x) * (point.y - start.y) - (end.y - start.y) * (point.x - start.x)
}

#[cfg(test)]
#[cfg(test)]
pub(super) fn point_to_clip(point: Point, view: CanvasView) -> [f32; 4] {
    transformed_point_to_clip(point, view.canvas_to_view)
}

#[cfg(test)]
fn transformed_point_to_clip(point: Point, canvas_to_view: Projective2) -> [f32; 4] {
    let view_point = canvas_to_view.transform_homogeneous(point);
    [
        2.0 * view_point.x - view_point.z,
        view_point.z - 2.0 * view_point.y,
        0.0,
        view_point.z,
    ]
}

pub(super) fn screen_space_tolerance(
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
