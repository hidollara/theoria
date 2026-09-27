use crate::{
    canvas::{
        ColorField, GradientExtend, GradientPoint, GradientStop, LinearGradient, MeshGradient,
    },
    geometry::Point,
};

use super::{
    WorkingColor,
    color_space::{PreparedColors, PreparedInterpolation},
    path_sampling::PathSampler,
};

#[cfg(test)]
mod tests;

#[cfg(test)]
use crate::tessellation::DEFAULT_TESSELLATION_TOLERANCE;

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq)]
struct TestWorkingColor([f32; 4]);

#[cfg(test)]
impl WorkingColor for TestWorkingColor {
    const TRANSPARENT: Self = Self([0.0; 4]);

    fn from_color(
        color: &crate::canvas::CanvasColor,
        prepared: &super::color_space::PreparedColors,
    ) -> Self {
        Self(prepared.lower(color))
    }

    fn from_components(components: [f32; 4]) -> Self {
        Self(components)
    }

    fn components(self) -> [f32; 4] {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PaintedTriangle<C: WorkingColor> {
    pub(crate) points: [Point; 3],
    pub(crate) colors: [C; 3],
}

#[derive(Clone, Copy)]
struct GradientVertex {
    point: Point,
    pub(super) offset: f32,
}

#[cfg(test)]
fn paint_triangle(
    points: [Point; 3],
    source: &ColorField,
) -> Vec<PaintedTriangle<TestWorkingColor>> {
    let mut prepared = PreparedColors::default();
    let profile = match source {
        ColorField::LinearGradient(gradient) => Some(gradient.interpolation_space()),
        ColorField::RadialGradient(gradient) => Some(gradient.interpolation_space()),
        ColorField::ConicGradient(gradient) => Some(gradient.interpolation_space()),
        ColorField::AlongPathGradient(gradient) => Some(gradient.ramp().interpolation_space()),
        ColorField::AcrossPathGradient(gradient) => Some(gradient.ramp().interpolation_space()),
        ColorField::MeshGradient(gradient) => Some(gradient.interpolation_space()),
        _ => None,
    };
    if let Some(profile) = profile {
        prepared.prepare_interpolation(profile).unwrap();
    }
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
    paint_triangle_prepared(points, source, path_sampler.as_ref(), &prepared)
}

pub(crate) fn paint_triangle_prepared<C: WorkingColor>(
    points: [Point; 3],
    source: &ColorField,
    path_sampler: Option<&PathSampler>,
    prepared: &PreparedColors,
) -> Vec<PaintedTriangle<C>> {
    match source {
        ColorField::Solid(color) => vec![PaintedTriangle {
            points,
            colors: [C::from_color(color, prepared); 3],
        }],
        ColorField::LinearGradient(gradient) => paint_linear_gradient(points, gradient, prepared),
        ColorField::RadialGradient(gradient) => {
            let stops = prepare_gradient_stops(gradient.stops(), prepared);
            let interpolation = prepared.interpolation(gradient.interpolation_space());
            paint_sampled_gradient(points, |point| {
                radial_parameter(point, gradient).map_or(C::TRANSPARENT, |parameter| {
                    let offset = extend_parameter(parameter, gradient.extend());
                    evaluate_gradient_stops(&stops, offset, interpolation)
                })
            })
        }
        ColorField::ConicGradient(gradient) => {
            let stops = prepare_gradient_stops(gradient.stops(), prepared);
            let interpolation = prepared.interpolation(gradient.interpolation_space());
            paint_sampled_gradient(points, |point| {
                conic_parameter(point, gradient).map_or(C::TRANSPARENT, |parameter| {
                    let offset = extend_parameter(parameter, gradient.extend());
                    evaluate_gradient_stops(&stops, offset, interpolation)
                })
            })
        }
        ColorField::AlongPathGradient(gradient) => {
            let sampler = path_sampler.expect("along-Path sources prepare their Path once");
            let stops = prepare_gradient_stops(gradient.ramp().stops(), prepared);
            let interpolation = prepared.interpolation(gradient.ramp().interpolation_space());
            paint_sampled_gradient(points, |point| {
                let offset = sampler.nearest_offset(point).unwrap_or(0.0);
                evaluate_gradient_stops(&stops, offset, interpolation)
            })
        }
        ColorField::AcrossPathGradient(gradient) => {
            let sampler = path_sampler.expect("across-Path sources prepare their Path once");
            let stops = prepare_gradient_stops(gradient.ramp().stops(), prepared);
            let interpolation = prepared.interpolation(gradient.ramp().interpolation_space());
            paint_sampled_gradient(points, |point| {
                let distance = sampler
                    .nearest_sample(point)
                    .map_or(gradient.from_distance(), |sample| sample.signed_distance);
                let offset = (distance - gradient.from_distance())
                    / (gradient.to_distance() - gradient.from_distance());
                evaluate_gradient_stops(&stops, offset, interpolation)
            })
        }
        ColorField::MeshGradient(gradient) => paint_mesh_gradient(points, gradient, prepared),
        ColorField::Image(_) => {
            unreachable!("image color fields use the sampled-image pipeline")
        }
        ColorField::Canvas(_) => {
            unreachable!("Canvas color fields are evaluated before mesh drawing")
        }
    }
}

fn conic_parameter(point: Point, gradient: &crate::canvas::ConicGradient) -> Option<f32> {
    let delta = point - gradient.center();
    let angle = delta.y.atan2(delta.x).rem_euclid(std::f32::consts::TAU);
    let sweep = gradient.end_angle() - gradient.start_angle();
    if sweep == 0.0 {
        return match gradient.extend() {
            GradientExtend::Pad => Some(if angle < gradient.start_angle() {
                0.0
            } else {
                1.0
            }),
            GradientExtend::Repeat | GradientExtend::Reflect => None,
        };
    }
    if (sweep.abs() - std::f32::consts::TAU).abs() <= f32::EPSILON * 8.0 {
        let directed = if sweep > 0.0 {
            (angle - gradient.start_angle()).rem_euclid(std::f32::consts::TAU)
        } else {
            -((gradient.start_angle() - angle).rem_euclid(std::f32::consts::TAU))
        };
        Some(directed / sweep)
    } else {
        // Partial sweeps use an absolute physical turn from 0 through TAU.
        // This preserves intentional discontinuities at the positive x-axis
        // and allows normalized COLRv1 endpoints outside that turn.
        Some((angle - gradient.start_angle()) / sweep)
    }
}

fn paint_linear_gradient<C: WorkingColor>(
    points: [Point; 3],
    gradient: &LinearGradient,
    prepared: &PreparedColors,
) -> Vec<PaintedTriangle<C>> {
    let interpolation = prepared.interpolation(gradient.interpolation_space());
    if gradient.extend() != GradientExtend::Pad || !interpolation.is_linear_builtin() {
        let axis = gradient.end() - gradient.start();
        let length_squared = axis.dot(axis);
        let stops = prepare_gradient_stops(gradient.stops(), prepared);
        return paint_sampled_gradient(points, |point| {
            let offset = (point - gradient.start()).dot(axis) / length_squared;
            evaluate_gradient_stops(
                &stops,
                extend_parameter(offset, gradient.extend()),
                interpolation,
            )
        });
    }
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

    let first = &stops[0];
    let before = clip_gradient_polygon(&remaining, first.offset(), true);
    append_gradient_polygon(
        &mut output,
        &before,
        [first.offset(); 2],
        [C::from_color(first.color(), prepared); 2],
        interpolation,
    );
    remaining = clip_gradient_polygon(&remaining, first.offset(), false);

    for pair in stops.windows(2) {
        let [from, to] = [&pair[0], &pair[1]];
        let band = clip_gradient_polygon(&remaining, to.offset(), true);
        append_gradient_polygon(
            &mut output,
            &band,
            [from.offset(), to.offset()],
            [
                C::from_color(from.color(), prepared),
                C::from_color(to.color(), prepared),
            ],
            interpolation,
        );
        remaining = clip_gradient_polygon(&remaining, to.offset(), false);
    }

    let last = stops.last().unwrap();
    append_gradient_polygon(
        &mut output,
        &remaining,
        [last.offset(); 2],
        [C::from_color(last.color(), prepared); 2],
        interpolation,
    );
    output
}

fn extend_parameter(value: f32, extend: GradientExtend) -> f32 {
    match extend {
        GradientExtend::Pad => value.clamp(0.0, 1.0),
        GradientExtend::Repeat => value.rem_euclid(1.0),
        GradientExtend::Reflect => {
            let repeated = value.rem_euclid(2.0);
            if repeated <= 1.0 {
                repeated
            } else {
                2.0 - repeated
            }
        }
    }
}

fn radial_parameter(point: Point, gradient: &crate::canvas::RadialGradient) -> Option<f32> {
    let centers = gradient.end_center() - gradient.start_center();
    let from_start = point - gradient.start_center();
    let radius_delta = gradient.end_radius() - gradient.start_radius();
    let a = centers.dot(centers) - radius_delta * radius_delta;
    let b = -2.0 * (from_start.dot(centers) + gradient.start_radius() * radius_delta);
    let c = from_start.dot(from_start) - gradient.start_radius() * gradient.start_radius();
    if a.abs() <= f32::EPSILON {
        return (b.abs() > f32::EPSILON).then_some(-c / b);
    }
    let discriminant = b.mul_add(b, -4.0 * a * c);
    if discriminant < 0.0 {
        return None;
    }
    let root = discriminant.sqrt();
    let first = (-b - root) / (2.0 * a);
    let second = (-b + root) / (2.0 * a);
    [first, second]
        .into_iter()
        .filter(|value| gradient.start_radius() + value * radius_delta >= 0.0)
        .max_by(f32::total_cmp)
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

fn append_gradient_polygon<C: WorkingColor>(
    output: &mut Vec<PaintedTriangle<C>>,
    polygon: &[GradientVertex],
    [start_offset, end_offset]: [f32; 2],
    [start_color, end_color]: [C; 2],
    interpolation: &PreparedInterpolation,
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
                start_color.interpolate(end_color, amount, interpolation)
            }),
        });
    }
}

fn prepare_gradient_stops<C: WorkingColor>(
    stops: &[GradientStop],
    prepared: &PreparedColors,
) -> Vec<(f32, C)> {
    stops
        .iter()
        .map(|stop| (stop.offset(), C::from_color(stop.color(), prepared)))
        .collect()
}

fn evaluate_gradient_stops<C: WorkingColor>(
    stops: &[(f32, C)],
    offset: f32,
    interpolation: &PreparedInterpolation,
) -> C {
    if offset <= stops[0].0 {
        return stops[0].1;
    }
    for pair in stops.windows(2) {
        let [from, to] = [&pair[0], &pair[1]];
        if offset < to.0 {
            let amount = (offset - from.0) / (to.0 - from.0);
            return from.1.interpolate(to.1, amount, interpolation);
        }
    }
    stops.last().unwrap().1
}

fn paint_sampled_gradient<C: WorkingColor>(
    points: [Point; 3],
    sample: impl Fn(Point) -> C + Copy,
) -> Vec<PaintedTriangle<C>> {
    let mut output = Vec::new();
    subdivide_sampled_gradient(points, sample, 0, &mut output);
    output
}

fn subdivide_sampled_gradient<C: WorkingColor>(
    points: [Point; 3],
    sample: impl Fn(Point) -> C + Copy,
    depth: u32,
    output: &mut Vec<PaintedTriangle<C>>,
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
        let first = colors[edge].components();
        let second = colors[(edge + 1) % 3].components();
        let expected = C::from_components(std::array::from_fn(|component| {
            (first[component] + second[component]) * 0.5
        }));
        error = error.max(color_difference(actual, expected));
    }
    let actual_center = sample(center);
    let component_colors = colors.map(WorkingColor::components);
    let expected_center = C::from_components(std::array::from_fn(|component| {
        (component_colors[0][component]
            + component_colors[1][component]
            + component_colors[2][component])
            / 3.0
    }));
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

fn color_difference<C: WorkingColor>(first: C, second: C) -> f32 {
    first.max_component_difference(second)
}

fn paint_mesh_gradient<C: WorkingColor>(
    points: [Point; 3],
    gradient: &MeshGradient,
    prepared: &PreparedColors,
) -> Vec<PaintedTriangle<C>> {
    let mut output = Vec::new();
    let interpolation = prepared.interpolation(gradient.interpolation_space());
    for patch in gradient.triangles() {
        let patch_points = std::array::from_fn(|index| patch[index].position());
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
            if interpolation.is_linear_builtin() {
                output.push(PaintedTriangle {
                    points: triangle_points,
                    colors: triangle_points
                        .map(|point| mesh_color_at(point, patch, interpolation, prepared)),
                });
            } else {
                output.extend(paint_sampled_gradient(triangle_points, |point| {
                    mesh_color_at(point, patch, interpolation, prepared)
                }));
            }
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

fn mesh_color_at<C: WorkingColor>(
    point: Point,
    patch: &[GradientPoint; 3],
    interpolation: &PreparedInterpolation,
    prepared: &PreparedColors,
) -> C {
    let positions = std::array::from_fn(|index| patch[index].position());
    let denominator = signed_area(positions);
    let weights = [
        edge_side(positions[1], positions[2], point) / denominator,
        edge_side(positions[2], positions[0], point) / denominator,
        edge_side(positions[0], positions[1], point) / denominator,
    ];
    let colors = std::array::from_fn(|index| C::from_color(patch[index].color(), prepared));
    C::mix(colors, weights, interpolation)
}

fn signed_area(points: [Point; 3]) -> f32 {
    edge_side(points[0], points[1], points[2])
}

fn edge_side(start: Point, end: Point, point: Point) -> f32 {
    (end.x - start.x) * (point.y - start.y) - (end.y - start.y) * (point.x - start.x)
}
