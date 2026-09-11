use super::super::image::GpuAlphaMode;
use super::canvas_plan::{can_record_directly, collect_images, required_scratch_images};
use super::encoding::ImagePushConstants;
use super::mesh_paint::{PathSampler, paint_triangle, point_to_clip, screen_space_tolerance};
use super::pipeline::composite_blend_factors;
use super::*;
use crate::canvas::{
    ColorField, Composite, ConicGradient, Coverage, GradientExtend, GradientPoint, GradientStop,
    ImagePattern, LinearGradient, MeshGradient, RadialGradient,
};
use crate::color::Color;
use crate::geometry::Point;
use crate::path::{Path, Subpath};
use crate::tessellation::DEFAULT_TESSELLATION_TOLERANCE;

#[test]
fn image_is_a_raster_boundary_while_canvas_color_field_stays_semantic() {
    let pixels = Image::from_rgba8(1, 1, vec![255; 4]).unwrap();
    let producer = Canvas::draw(pixels.clone(), Coverage::all());
    let view = CanvasView::axis_aligned(Point::new(0.0, 0.0), Point::new(1.0, 1.0)).unwrap();
    let image = Image::from_canvas(1, 1, producer.clone(), view).unwrap();

    let mut image_dependencies = Vec::new();
    collect_images(
        &Canvas::draw(image.clone(), Coverage::all()),
        &mut image_dependencies,
    );
    assert_eq!(image_dependencies.len(), 1);
    assert_eq!(image_dependencies[0].instance_key(), image.instance_key());

    let mut canvas_dependencies = Vec::new();
    collect_images(
        &Canvas::draw(producer, Coverage::all()),
        &mut canvas_dependencies,
    );
    assert_eq!(canvas_dependencies.len(), 1);
    assert_eq!(canvas_dependencies[0].instance_key(), pixels.instance_key());
}

#[test]
fn image_sampling_respects_the_gpu_images_alpha_representation() {
    let straight = Image::from_rgba8(1, 1, vec![255, 128, 64, 128]).unwrap();
    let premultiplied = Image::from_canvas(
        1,
        1,
        Canvas::empty(),
        CanvasView::axis_aligned(Point::new(0.0, 0.0), Point::new(1.0, 1.0)).unwrap(),
    )
    .unwrap();
    let points = [Point::new(0.0, 0.0); 3];
    let view = CanvasView::axis_aligned(Point::new(0.0, 0.0), Point::new(1.0, 1.0)).unwrap();

    let straight_constants = ImagePushConstants::new(
        &ImagePattern::new(straight),
        GpuAlphaMode::Straight,
        points,
        Projective2::IDENTITY,
        view,
    );
    let premultiplied_constants = ImagePushConstants::new(
        &ImagePattern::new(premultiplied),
        GpuAlphaMode::Premultiplied,
        points,
        Projective2::IDENTITY,
        view,
    );

    assert_eq!(straight_constants.vertices[0].uv[3], 0.0);
    assert_eq!(premultiplied_constants.vertices[0].uv[3], 1.0);
}

#[test]
fn four_sample_msaa_is_preferred_with_a_single_sample_fallback() {
    assert_eq!(
        choose_sample_count(
            vk::SampleCountFlags::TYPE_1
                | vk::SampleCountFlags::TYPE_2
                | vk::SampleCountFlags::TYPE_4,
        ),
        vk::SampleCountFlags::TYPE_4
    );
    assert_eq!(
        choose_sample_count(vk::SampleCountFlags::TYPE_1 | vk::SampleCountFlags::TYPE_2),
        vk::SampleCountFlags::TYPE_1
    );
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
fn only_coverage_boundaries_reserve_independent_results() {
    let masked = Canvas::draw(Canvas::empty(), Coverage::from_alpha(Canvas::empty()))
        .transformed(Projective2::IDENTITY);
    let root = Canvas::empty().composite(masked, Composite::SourceOver);

    assert_eq!(required_scratch_images(&root), 3);
}

#[test]
fn non_source_over_composites_create_group_boundaries() {
    let leaf = Canvas::draw(Color::rgb8(255, 255, 255), Coverage::all());
    let grouped = leaf.clone().composite(leaf, Composite::SourceIn);
    let root = Canvas::empty().composite(grouped.clone(), Composite::SourceOver);

    assert!(!can_record_directly(&grouped));
    assert_eq!(required_scratch_images(&grouped), 2);
    assert_eq!(required_scratch_images(&root), 3);
}

#[test]
fn destination_reading_blends_reserve_source_and_backdrop_images() {
    let backdrop = Canvas::draw(Color::rgb8(64, 128, 255), Coverage::all());
    let source = Canvas::draw(Color::rgb8(255, 128, 64), Coverage::all());
    let blended = backdrop.blend_composite(
        source,
        crate::canvas::BlendMode::Multiply,
        Composite::SourceOver,
    );

    assert!(!can_record_directly(&blended));
    assert_eq!(required_scratch_images(&blended), 3);
}

#[test]
fn porter_duff_operators_use_premultiplied_alpha_factors() {
    let zero = vk::BlendFactor::ZERO;
    let one = vk::BlendFactor::ONE;
    let source_alpha = vk::BlendFactor::SRC_ALPHA;
    let destination_alpha = vk::BlendFactor::DST_ALPHA;
    let one_minus_source_alpha = vk::BlendFactor::ONE_MINUS_SRC_ALPHA;
    let one_minus_destination_alpha = vk::BlendFactor::ONE_MINUS_DST_ALPHA;

    let expected = [
        (Composite::Clear, (zero, zero)),
        (Composite::Copy, (one, zero)),
        (Composite::Destination, (zero, one)),
        (Composite::SourceOver, (one, one_minus_source_alpha)),
        (
            Composite::DestinationOver,
            (one_minus_destination_alpha, one),
        ),
        (Composite::SourceIn, (destination_alpha, zero)),
        (Composite::DestinationIn, (zero, source_alpha)),
        (Composite::SourceOut, (one_minus_destination_alpha, zero)),
        (Composite::DestinationOut, (zero, one_minus_source_alpha)),
        (
            Composite::SourceAtop,
            (destination_alpha, one_minus_source_alpha),
        ),
        (
            Composite::DestinationAtop,
            (one_minus_destination_alpha, source_alpha),
        ),
        (
            Composite::Xor,
            (one_minus_destination_alpha, one_minus_source_alpha),
        ),
    ];

    for (operator, factors) in expected {
        assert_eq!(composite_blend_factors(operator), factors);
    }

    assert_eq!(
        composite_blend_factors(Composite::Plus),
        (vk::BlendFactor::ONE, vk::BlendFactor::ONE)
    );
}

#[test]
fn along_path_offsets_restart_for_each_subpath() {
    let path = Path::from_subpaths([
        Subpath::new(Point::new(0.0, 0.0)).line_to(Point::new(10.0, 0.0)),
        Subpath::new(Point::new(0.0, 20.0)).line_to(Point::new(100.0, 20.0)),
    ]);
    let sampler = PathSampler::new(&path, DEFAULT_TESSELLATION_TOLERANCE);

    assert!((sampler.nearest_offset(Point::new(2.5, 1.0)).unwrap() - 0.25).abs() < 0.001);
    assert!((sampler.nearest_offset(Point::new(25.0, 19.0)).unwrap() - 0.25).abs() < 0.001);
    assert!(
        (sampler
            .nearest_sample(Point::new(2.5, 1.0))
            .unwrap()
            .signed_distance
            - 1.0)
            .abs()
            < 0.001
    );
    assert!(
        (sampler
            .nearest_sample(Point::new(25.0, 19.0))
            .unwrap()
            .signed_distance
            + 1.0)
            .abs()
            < 0.001
    );
}

#[test]
fn converts_canvas_corners_to_clip_space() {
    let view = CanvasView::axis_aligned(Point::new(0.0, 0.0), Point::new(800.0, 600.0)).unwrap();
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
    let view = CanvasView::axis_aligned(Point::new(100.0, 50.0), Point::new(200.0, 100.0)).unwrap();
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

#[test]
fn linear_gradient_splits_triangles_at_stop_boundaries() {
    let green = Color::rgb8(0, 255, 0).to_premultiplied_array();
    let gradient = LinearGradient::new(
        Point::new(0.0, 0.0),
        Point::new(10.0, 0.0),
        [
            GradientStop::new(0.0, Color::rgb8(255, 0, 0)).unwrap(),
            GradientStop::new(0.5, Color::rgb8(0, 255, 0)).unwrap(),
            GradientStop::new(1.0, Color::rgb8(0, 0, 255)).unwrap(),
        ],
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
    let red = Color::rgb8(255, 0, 0).to_premultiplied_array();
    let gradient = LinearGradient::new(
        Point::new(0.0, 0.0),
        Point::new(10.0, 0.0),
        [
            GradientStop::new(0.2, Color::rgb8(255, 0, 0)).unwrap(),
            GradientStop::new(0.8, Color::rgb8(0, 0, 255)).unwrap(),
        ],
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
    let gradient = MeshGradient::from_triangle(std::array::from_fn(|index| {
        GradientPoint::new(positions[index], colors[index])
    }))
    .unwrap();
    let triangles = paint_triangle(positions, &ColorField::MeshGradient(gradient));

    assert_eq!(triangles.len(), 1);
    for (point, color) in positions.into_iter().zip(colors) {
        let index = triangles[0]
            .points
            .iter()
            .position(|candidate| *candidate == point)
            .unwrap();
        assert_eq!(triangles[0].colors[index], color.to_premultiplied_array());
    }
}

#[test]
fn mesh_gradient_clips_painted_geometry_to_its_patch() {
    let patch = [
        GradientPoint::new(Point::new(0.0, 0.0), Color::rgb8(255, 0, 0)),
        GradientPoint::new(Point::new(4.0, 0.0), Color::rgb8(0, 255, 0)),
        GradientPoint::new(Point::new(0.0, 4.0), Color::rgb8(0, 0, 255)),
    ];
    let gradient = MeshGradient::from_triangle(patch).unwrap();
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
            .any(|(point, color)| {
                *point == Point::new(0.0, 0.0) && color == red.to_premultiplied_array()
            })
    }));
    assert!(triangles.iter().any(|triangle| {
        triangle
            .points
            .iter()
            .zip(triangle.colors)
            .any(|(point, color)| {
                *point == Point::new(10.0, 0.0) && color == blue.to_premultiplied_array()
            })
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
            .all(|color| color == [0.0; 4])
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
            .all(|color| color == [0.0; 4])
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
                .any(|(candidate, color)| {
                    *candidate == point && color == blue.to_premultiplied_array()
                })
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
            .any(|(point, color)| {
                *point == Point::new(1.0, 0.0) && color == red.to_premultiplied_array()
            })
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
            .any(|(point, color)| {
                *point == Point::new(10.0, 0.0) && color == red.to_premultiplied_array()
            })
    }));
    assert!(triangles.iter().any(|triangle| {
        triangle
            .points
            .iter()
            .zip(triangle.colors)
            .any(|(point, color)| {
                *point == Point::new(-10.0, 0.0) && color == green.to_premultiplied_array()
            })
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
    assert_eq!(
        color_at(Point::new(10.0, 0.0)),
        Some(red.to_premultiplied_array())
    );
    assert_eq!(
        color_at(Point::new(-10.0, 0.0)),
        Some(blue.to_premultiplied_array())
    );
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
            .any(|(point, color)| {
                *point == Point::new(10.0, 0.0) && color == red.to_premultiplied_array()
            })
    }));
    assert!(triangles.iter().any(|triangle| {
        triangle
            .points
            .iter()
            .zip(triangle.colors)
            .any(|(point, color)| {
                *point == Point::new(-10.0, 0.0) && color == blue.to_premultiplied_array()
            })
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
            .all(|color| color == [0.0; 4])
    );
}
