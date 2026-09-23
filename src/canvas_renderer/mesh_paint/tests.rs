use super::{TestWorkingColor, paint_triangle};
use crate::{
    canvas::{
        ColorField, ConicGradient, GradientExtend, GradientPoint, GradientStop, LinearGradient,
        MeshGradient, RadialGradient,
    },
    canvas_renderer::WorkingColor,
    color::{Color, LinearSrgb},
    geometry::Point,
};

fn lowered(color: impl Into<crate::canvas::CanvasColor>) -> TestWorkingColor {
    TestWorkingColor::from_color(&color.into())
}

#[test]
fn a_transparent_field_produces_no_painted_geometry() {
    assert!(paint_triangle([Point::new(0.0, 0.0); 3], &ColorField::Transparent).is_empty());
}

#[test]
fn linear_gradient_splits_triangles_at_stop_boundaries() {
    let green = lowered(Color::rgb8(0, 255, 0));
    let gradient = LinearGradient::new(
        Point::new(0.0, 0.0),
        Point::new(10.0, 0.0),
        [
            GradientStop::new(0.0, Color::rgb8(255, 0, 0)).unwrap(),
            GradientStop::new(0.5, Color::rgb8(0, 255, 0)).unwrap(),
            GradientStop::new(1.0, Color::rgb8(0, 0, 255)).unwrap(),
        ],
        LinearSrgb,
    )
    .unwrap();
    let triangles = paint_triangle(
        [
            Point::new(0.0, 0.0),
            Point::new(10.0, 0.0),
            Point::new(0.0, 10.0),
        ],
        &ColorField::LinearGradient(gradient),
    );

    assert!(triangles.len() > 1);
    assert!(
        triangles
            .iter()
            .flat_map(|triangle| triangle.colors)
            .any(|color| color == green)
    );
    for triangle in triangles {
        let min = triangle
            .points
            .iter()
            .map(|point| point.x / 10.0)
            .fold(f32::INFINITY, f32::min);
        let max = triangle
            .points
            .iter()
            .map(|point| point.x / 10.0)
            .fold(f32::NEG_INFINITY, f32::max);
        assert!(!(min < 0.5 - 0.0001 && max > 0.5 + 0.0001));
    }
}

#[test]
fn linear_gradient_clamps_colors_outside_its_axis() {
    let red = lowered(Color::rgb8(255, 0, 0));
    let gradient = LinearGradient::new(
        Point::new(0.0, 0.0),
        Point::new(10.0, 0.0),
        [
            GradientStop::new(0.2, Color::rgb8(255, 0, 0)).unwrap(),
            GradientStop::new(0.8, Color::rgb8(0, 0, 255)).unwrap(),
        ],
        LinearSrgb,
    )
    .unwrap();
    let triangles = paint_triangle(
        [
            Point::new(-10.0, 0.0),
            Point::new(-2.0, 0.0),
            Point::new(-10.0, 8.0),
        ],
        &ColorField::LinearGradient(gradient),
    );

    assert_eq!(triangles.len(), 1);
    assert_eq!(triangles[0].colors, [red; 3]);
}

#[test]
fn nonlinear_gradient_interpolation_is_resolved_before_gpu_interpolation() {
    let gradient = LinearGradient::new(
        Point::new(0.0, 0.0),
        Point::new(10.0, 0.0),
        [
            GradientStop::new(0.0, Color::rgb8(0, 0, 0)).unwrap(),
            GradientStop::new(1.0, Color::rgb8(255, 255, 255)).unwrap(),
        ],
        crate::color::Srgb,
    )
    .unwrap();
    let triangles = paint_triangle(
        [
            Point::new(0.0, 0.0),
            Point::new(10.0, 0.0),
            Point::new(0.0, 10.0),
        ],
        &ColorField::LinearGradient(gradient),
    );
    let midpoint = triangles.iter().find_map(|triangle| {
        triangle
            .points
            .iter()
            .position(|point| *point == Point::new(5.0, 0.0))
            .map(|index| triangle.colors[index].components()[0])
    });

    assert!(triangles.len() > 1);
    assert!((midpoint.unwrap() - 0.214_041_14).abs() < 0.000_001);
}

#[test]
fn triangular_mesh_gradient_reproduces_its_vertex_colors() {
    let positions = [
        Point::new(0.0, 0.0),
        Point::new(10.0, 0.0),
        Point::new(0.0, 10.0),
    ];
    let colors = [
        Color::rgb8(255, 0, 0),
        Color::rgb8(0, 255, 0),
        Color::rgb8(0, 0, 255),
    ];
    let gradient = MeshGradient::from_triangle(
        std::array::from_fn(|index| GradientPoint::new(positions[index], colors[index])),
        LinearSrgb,
    )
    .unwrap();
    let triangles = paint_triangle(positions, &ColorField::MeshGradient(gradient));

    assert_eq!(triangles.len(), 1);
    for (point, color) in positions.into_iter().zip(colors) {
        let index = triangles[0]
            .points
            .iter()
            .position(|candidate| *candidate == point)
            .unwrap();
        assert_eq!(triangles[0].colors[index], lowered(color));
    }
}

#[test]
fn mesh_gradient_clips_painted_geometry_to_its_patch() {
    let patch = [
        GradientPoint::new(Point::new(0.0, 0.0), Color::rgb8(255, 0, 0)),
        GradientPoint::new(Point::new(4.0, 0.0), Color::rgb8(0, 255, 0)),
        GradientPoint::new(Point::new(0.0, 4.0), Color::rgb8(0, 0, 255)),
    ];
    let gradient = MeshGradient::from_triangle(patch, LinearSrgb).unwrap();
    let triangles = paint_triangle(
        [
            Point::new(-10.0, -10.0),
            Point::new(20.0, -10.0),
            Point::new(-10.0, 20.0),
        ],
        &ColorField::MeshGradient(gradient),
    );

    assert!(!triangles.is_empty());
    assert!(
        triangles
            .iter()
            .flat_map(|triangle| triangle.points)
            .all(|point| point.x >= -0.0001 && point.y >= -0.0001 && point.x + point.y <= 4.0001)
    );
}

#[test]
fn radial_gradient_samples_center_and_radius_colors() {
    let red = Color::rgb8(255, 0, 0);
    let blue = Color::rgb8(0, 0, 255);
    let gradient = RadialGradient::new(
        Point::new(0.0, 0.0),
        10.0,
        [
            GradientStop::new(0.0, red).unwrap(),
            GradientStop::new(1.0, blue).unwrap(),
        ],
        LinearSrgb,
    )
    .unwrap();
    let triangles = paint_triangle(
        [
            Point::new(0.0, 0.0),
            Point::new(10.0, 0.0),
            Point::new(0.0, 10.0),
        ],
        &ColorField::RadialGradient(gradient),
    );

    assert!(triangles.len() > 16);
    assert!(triangles.iter().any(|triangle| {
        triangle
            .points
            .iter()
            .zip(triangle.colors)
            .any(|(point, color)| *point == Point::new(0.0, 0.0) && color == lowered(red))
    }));
    assert!(triangles.iter().any(|triangle| {
        triangle
            .points
            .iter()
            .zip(triangle.colors)
            .any(|(point, color)| *point == Point::new(10.0, 0.0) && color == lowered(blue))
    }));
}

#[test]
fn two_circle_radial_gradient_is_transparent_where_no_circle_reaches() {
    let gradient = RadialGradient::between_circles(
        Point::new(0.0, 0.0),
        1.0,
        Point::new(10.0, 0.0),
        1.0,
        [
            GradientStop::new(0.0, Color::rgb8(255, 0, 0)).unwrap(),
            GradientStop::new(1.0, Color::rgb8(0, 0, 255)).unwrap(),
        ],
        LinearSrgb,
    )
    .unwrap();
    let triangles = paint_triangle(
        [
            Point::new(0.0, 100.0),
            Point::new(1.0, 100.0),
            Point::new(0.0, 101.0),
        ],
        &ColorField::RadialGradient(gradient),
    );

    assert!(
        triangles
            .iter()
            .flat_map(|triangle| triangle.colors)
            .all(|color| color == TestWorkingColor::TRANSPARENT)
    );
}

#[test]
fn coincident_radial_circles_paint_nothing() {
    let gradient = RadialGradient::between_circles(
        Point::new(2.0, 3.0),
        4.0,
        Point::new(2.0, 3.0),
        4.0,
        [
            GradientStop::new(0.0, Color::rgb8(255, 0, 0)).unwrap(),
            GradientStop::new(1.0, Color::rgb8(0, 0, 255)).unwrap(),
        ],
        LinearSrgb,
    )
    .unwrap();
    let triangles = paint_triangle(
        [
            Point::new(0.0, 0.0),
            Point::new(10.0, 0.0),
            Point::new(0.0, 10.0),
        ],
        &ColorField::RadialGradient(gradient),
    );

    assert!(
        triangles
            .iter()
            .flat_map(|triangle| triangle.colors)
            .all(|color| color == TestWorkingColor::TRANSPARENT)
    );
}

#[test]
fn signed_radial_gradient_uses_only_nonnegative_radius_solutions() {
    let red = Color::rgb8(255, 0, 0);
    let blue = Color::rgb8(0, 0, 255);
    let gradient = RadialGradient::between_circles(
        Point::new(0.0, 0.0),
        -1.0,
        Point::new(0.0, 0.0),
        1.0,
        [
            GradientStop::new(0.0, red).unwrap(),
            GradientStop::new(1.0, blue).unwrap(),
        ],
        LinearSrgb,
    )
    .unwrap();
    let triangles = paint_triangle(
        [
            Point::new(1.0, 0.0),
            Point::new(0.0, 1.0),
            Point::new(-1.0, 0.0),
        ],
        &ColorField::RadialGradient(gradient),
    );

    for point in [
        Point::new(1.0, 0.0),
        Point::new(0.0, 1.0),
        Point::new(-1.0, 0.0),
    ] {
        assert!(triangles.iter().any(|triangle| {
            triangle
                .points
                .iter()
                .zip(triangle.colors)
                .any(|(candidate, color)| *candidate == point && color == lowered(blue))
        }));
    }
}

#[test]
fn negative_radial_endpoints_can_reach_a_positive_extended_cone() {
    let red = Color::rgb8(255, 0, 0);
    let gradient = RadialGradient::between_circles(
        Point::new(0.0, 0.0),
        -1.0,
        Point::new(0.0, 0.0),
        -2.0,
        [
            GradientStop::new(0.0, red).unwrap(),
            GradientStop::new(1.0, Color::rgb8(0, 0, 255)).unwrap(),
        ],
        LinearSrgb,
    )
    .unwrap();
    let triangles = paint_triangle(
        [
            Point::new(1.0, 0.0),
            Point::new(0.0, 1.0),
            Point::new(-1.0, 0.0),
        ],
        &ColorField::RadialGradient(gradient),
    );

    assert!(triangles.iter().any(|triangle| {
        triangle
            .points
            .iter()
            .zip(triangle.colors)
            .any(|(point, color)| *point == Point::new(1.0, 0.0) && color == lowered(red))
    }));
}

#[test]
fn conic_gradient_uses_positive_parameter_angle() {
    let red = Color::rgb8(255, 0, 0);
    let green = Color::rgb8(0, 255, 0);
    let blue = Color::rgb8(0, 0, 255);
    let gradient = ConicGradient::new(
        Point::new(0.0, 0.0),
        0.0,
        [
            GradientStop::new(0.0, red).unwrap(),
            GradientStop::new(0.5, green).unwrap(),
            GradientStop::new(1.0, blue).unwrap(),
        ],
        LinearSrgb,
    )
    .unwrap();
    let triangles = paint_triangle(
        [
            Point::new(10.0, 0.0),
            Point::new(-10.0, 0.0),
            Point::new(0.0, 10.0),
        ],
        &ColorField::ConicGradient(gradient),
    );

    assert!(triangles.len() > 16);
    assert!(triangles.iter().any(|triangle| {
        triangle
            .points
            .iter()
            .zip(triangle.colors)
            .any(|(point, color)| *point == Point::new(10.0, 0.0) && color == lowered(red))
    }));
    assert!(triangles.iter().any(|triangle| {
        triangle
            .points
            .iter()
            .zip(triangle.colors)
            .any(|(point, color)| *point == Point::new(-10.0, 0.0) && color == lowered(green))
    }));
}

#[test]
fn partial_conic_gradient_pads_on_the_absolute_turn() {
    let red = Color::rgb8(255, 0, 0);
    let blue = Color::rgb8(0, 0, 255);
    let gradient = ConicGradient::sweeping(
        Point::new(0.0, 0.0),
        std::f32::consts::FRAC_PI_2,
        std::f32::consts::PI,
        [
            GradientStop::new(0.0, red).unwrap(),
            GradientStop::new(1.0, blue).unwrap(),
        ],
        LinearSrgb,
    )
    .unwrap();
    let triangles = paint_triangle(
        [
            Point::new(10.0, 0.0),
            Point::new(0.0, 10.0),
            Point::new(-10.0, 0.0),
        ],
        &ColorField::ConicGradient(gradient),
    );

    let color_at = |point| {
        triangles.iter().find_map(|triangle| {
            triangle
                .points
                .iter()
                .position(|candidate| *candidate == point)
                .map(|index| triangle.colors[index])
        })
    };
    assert_eq!(color_at(Point::new(10.0, 0.0)), Some(lowered(red)));
    assert_eq!(color_at(Point::new(-10.0, 0.0)), Some(lowered(blue)));
}

#[test]
fn coincident_conic_angles_form_a_pad_step() {
    let red = Color::rgb8(255, 0, 0);
    let blue = Color::rgb8(0, 0, 255);
    let gradient = ConicGradient::sweeping(
        Point::new(0.0, 0.0),
        std::f32::consts::FRAC_PI_2,
        std::f32::consts::FRAC_PI_2,
        [
            GradientStop::new(0.0, red).unwrap(),
            GradientStop::new(1.0, blue).unwrap(),
        ],
        LinearSrgb,
    )
    .unwrap();
    let triangles = paint_triangle(
        [
            Point::new(10.0, 0.0),
            Point::new(-10.0, 0.0),
            Point::new(0.0, 10.0),
        ],
        &ColorField::ConicGradient(gradient),
    );

    assert!(triangles.iter().any(|triangle| {
        triangle
            .points
            .iter()
            .zip(triangle.colors)
            .any(|(point, color)| *point == Point::new(10.0, 0.0) && color == lowered(red))
    }));
    assert!(triangles.iter().any(|triangle| {
        triangle
            .points
            .iter()
            .zip(triangle.colors)
            .any(|(point, color)| *point == Point::new(-10.0, 0.0) && color == lowered(blue))
    }));
}

#[test]
fn coincident_repeating_conic_angles_paint_nothing() {
    let gradient = ConicGradient::sweeping(
        Point::new(0.0, 0.0),
        0.0,
        0.0,
        [
            GradientStop::new(0.0, Color::rgb8(255, 0, 0)).unwrap(),
            GradientStop::new(1.0, Color::rgb8(0, 0, 255)).unwrap(),
        ],
        LinearSrgb,
    )
    .unwrap()
    .with_extend(GradientExtend::Repeat);
    let triangles = paint_triangle(
        [
            Point::new(0.0, 0.0),
            Point::new(10.0, 0.0),
            Point::new(0.0, 10.0),
        ],
        &ColorField::ConicGradient(gradient),
    );

    assert!(
        triangles
            .iter()
            .flat_map(|triangle| triangle.colors)
            .all(|color| color == TestWorkingColor::TRANSPARENT)
    );
}
