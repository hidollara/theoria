use std::sync::Arc;

use crate::{
    canvas::{Color, Opacity},
    color::{ColorSpace, DisplayP3, LinearSrgb, Srgb},
    geometry::Point,
    image::{AlphaAssociation, RasterImage, RasterPlane, Rgba8, Srgba8Image},
    math::Vector2,
    path::{
        DashPattern, DashPatternError, FillRule, LineCap, LineJoin, Path, StrokeStyle,
        StrokeStyleError,
    },
    transform::{Affine2, Projective2},
};

use super::*;

fn filled_triangle(points: [Point; 3], source: ColorField) -> Canvas {
    Canvas::draw(source, Coverage::fill(Path::polygon(points)))
}

fn sample_canvas() -> Canvas {
    Canvas::draw(
        Color::new(Srgb, [10_u8, 20, 30], Opacity::<u8>::ONE),
        Coverage::all(),
    )
}

fn assert_point_near(actual: Option<Point>, expected: Point) {
    let actual = actual.unwrap();
    assert!((actual.x - expected.x).abs() < 0.001);
    assert!((actual.y - expected.y).abs() < 0.001);
}

#[test]
fn canvas_image_fixes_a_canvas_view_and_pixel_extent() {
    let content = Canvas::draw(
        Color::new(Srgb, [10_u8, 20, 30], Opacity::<u8>::ONE),
        Coverage::all(),
    );
    let view = CanvasView::axis_aligned(Point::new(-2.0, -1.0), Point::new(2.0, 1.0)).unwrap();
    let image = CanvasImage::new(content.clone(), view, 320, 180).unwrap();

    assert_eq!(image.view(), view);
    assert_eq!(image.width(), 320);
    assert_eq!(image.height(), 180);
    assert!(Arc::ptr_eq(&image.content().node, &content.node));
}

#[test]
fn canvas_image_converts_to_a_finite_image() {
    let view = CanvasView::axis_aligned(Point::new(-2.0, -1.0), Point::new(2.0, 1.0)).unwrap();
    let canvas_image = CanvasImage::new(sample_canvas(), view, 320, 180).unwrap();
    let image = Image::from(canvas_image);

    assert_eq!(image.width(), 320);
    assert_eq!(image.height(), 180);
}

#[test]
fn canvas_image_clones_share_one_materialization_identity() {
    let view = CanvasView::axis_aligned(Point::new(0.0, 0.0), Point::new(1.0, 1.0)).unwrap();
    let image = CanvasImage::new(sample_canvas(), view, 1, 1).unwrap();
    let clone = image.clone();
    let independent = CanvasImage::new(sample_canvas(), view, 1, 1).unwrap();

    assert_eq!(image.instance_key(), clone.instance_key());
    assert_ne!(image.instance_key(), independent.instance_key());
}

#[test]
fn canvas_image_cache_identity_does_not_keep_the_image_alive() {
    let key = {
        let view = CanvasView::axis_aligned(Point::new(0.0, 0.0), Point::new(1.0, 1.0)).unwrap();
        CanvasImage::new(sample_canvas(), view, 1, 1)
            .unwrap()
            .instance_key()
    };

    assert!(!key.is_alive());
}

#[test]
fn canvas_image_rejects_an_empty_pixel_extent() {
    let view = CanvasView::axis_aligned(Point::new(0.0, 0.0), Point::new(1.0, 1.0)).unwrap();

    assert_eq!(
        CanvasImage::new(sample_canvas(), view, 0, 10).unwrap_err(),
        CanvasImageError::Empty {
            width: 0,
            height: 10,
        }
    );
}

#[test]
fn composes_color_fields_in_paint_order_without_a_size() {
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

    let first = filled_triangle(
        first,
        ColorField::Solid(Color::new(Srgb, [1.0_f32, 0.0, 0.0], Opacity::<f32>::ONE).into()),
    );
    let second = filled_triangle(
        second,
        ColorField::Solid(Color::new(Srgb, [0.0_f32, 1.0, 0.0], Opacity::<f32>::ONE).into()),
    );
    let canvas = first.clone().composite(
        second.clone(),
        Composition::new(Composite::SourceOver, LinearSrgb),
    );

    assert!(matches!(canvas.node(), CanvasNode::Composite {
        backdrop,
        source,
        composition,
    } if Arc::ptr_eq(&source.node, &second.node)
        && composition.blend().is_none()
        && composition.composite() == Composite::SourceOver
        && composition.composite_space() == &LinearSrgb.into()
        && Arc::ptr_eq(&backdrop.node, &first.node)));
}

#[test]
fn fill_records_non_zero_by_default_and_accepts_even_odd_explicitly() {
    let path = Path::polygon([
        Point::new(0.0, 0.0),
        Point::new(2.0, 0.0),
        Point::new(1.0, 2.0),
    ]);
    let source = ColorField::Solid(Color::new(Srgb, [255_u8, 255, 255], Opacity::<u8>::ONE).into());
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
fn an_image_can_be_used_as_an_unbounded_color_field_explicitly() {
    let image = Srgba8Image::new(2, 3, vec![255; 2 * 3 * 4]).unwrap();
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
    assert!(matches!(source, ColorField::Image(pattern)
            if pattern.image().width() == 2 && pattern.image().height() == 3));
}

#[test]
fn canvas_retains_typed_rgba8_raster_metadata_until_renderer_lowering() {
    let image = RasterImage::<Rgba8, DisplayP3>::from_planes(
        1,
        1,
        vec![RasterPlane::new(4, vec![255, 128, 64, 128])],
        DisplayP3,
        AlphaAssociation::Premultiplied,
    )
    .unwrap();
    let field = ColorField::from(image.clone());
    let ColorField::Image(pattern) = field else {
        panic!("an RGBA8 raster should become an image field");
    };
    let image::ImageKind::Raster(source) = pattern.image().kind() else {
        panic!("the image should retain a raster source");
    };
    let image::RasterImageView::Rgba8 {
        plane,
        color_space,
        alpha,
    } = source.view()
    else {
        panic!("the source should retain its RGBA format");
    };

    assert_eq!(source.instance_key(), image.instance_key());
    assert_eq!(plane.samples(), &[255, 128, 64, 128]);
    assert_eq!(color_space, ColorSpace::from(DisplayP3));
    assert_eq!(alpha, AlphaAssociation::Premultiplied);
}

#[test]
fn canvas_retains_an_opaque_rgb8_raster_without_inventing_alpha_metadata() {
    let image = RasterImage::<crate::image::Rgb8, DisplayP3>::from_planes(
        1,
        1,
        vec![RasterPlane::new(3, vec![255, 128, 64])],
        DisplayP3,
        (),
    )
    .unwrap();
    let field = ColorField::from(image);
    let ColorField::Image(pattern) = field else {
        panic!("an RGB8 raster should become an image field");
    };
    let image::ImageKind::Raster(source) = pattern.image().kind() else {
        panic!("the image should retain a raster source");
    };
    let image::RasterImageView::Rgb8 { plane, color_space } = source.view() else {
        panic!("the source should retain its RGB-only format");
    };

    assert_eq!(plane.samples(), &[255, 128, 64]);
    assert_eq!(color_space, ColorSpace::from(DisplayP3));
}

#[test]
fn image_pattern_separates_placement_sampling_and_extension() {
    let image = Srgba8Image::new(2, 3, vec![255; 2 * 3 * 4]).unwrap();
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
fn image_pattern_defaults_to_a_transparent_linear_color_field() {
    let image = Srgba8Image::new(1, 1, vec![255; 4]).unwrap();
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
    let source = ColorField::Solid(Color::new(Srgb, [255_u8, 255, 255], Opacity::<u8>::ONE).into());
    let base = filled_triangle(triangle, source.clone());
    let extended = base.clone().composite(
        filled_triangle(triangle, source),
        Composition::new(Composite::SourceOver, LinearSrgb),
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
    let child = filled_triangle(
        triangle,
        ColorField::Solid(Color::new(Srgb, [255_u8, 255, 255], Opacity::<u8>::ONE).into()),
    );
    let child_to_parent =
        Projective2::from_rows([[1.0, 0.0, 20.0], [0.0, 1.0, 30.0], [0.001, 0.0, 1.0]]);
    let transformed = child.clone().transformed(child_to_parent);

    assert!(matches!(transformed.node(), CanvasNode::Transform {
        content,
        transform,
    } if Arc::ptr_eq(&content.node, &child.node) && *transform == child_to_parent));
}

#[test]
fn coverage_pairs_a_lazy_canvas_color_field_with_a_canvas_alpha_field() {
    let triangle = [
        Point::new(0.0, 0.0),
        Point::new(1.0, 0.0),
        Point::new(0.0, 1.0),
    ];
    let source = ColorField::Solid(Color::new(Srgb, [255_u8, 255, 255], Opacity::<u8>::ONE).into());
    let content = filled_triangle(triangle, source.clone());
    let coverage_canvas = filled_triangle(triangle, source.clone());
    let overlay = filled_triangle(triangle, source);
    let masked = Canvas::draw(
        content.clone(),
        Coverage::from_alpha(coverage_canvas.clone()),
    );
    let canvas = masked.composite(overlay, Composition::new(Composite::SourceOver, LinearSrgb));

    assert!(matches!(canvas.node(), CanvasNode::Composite {
        backdrop,
        ..
    } if matches!(backdrop.node(), CanvasNode::Draw(Draw {
        source: ColorField::Canvas(recorded_content),
        coverage: Coverage {
            node: CoverageNode::AlphaOf(ColorField::Canvas(recorded_mask))
        },
        ..
    }) if Arc::ptr_eq(&recorded_content.node, &content.node)
        && Arc::ptr_eq(&recorded_mask.node, &coverage_canvas.node))));
}

#[test]
fn any_color_field_can_supply_an_alpha_coverage_field() {
    let triangle = Path::polygon([
        Point::new(0.0, 0.0),
        Point::new(1.0, 0.0),
        Point::new(0.0, 1.0),
    ]);
    let content = Canvas::draw(
        Color::new(Srgb, [255_u8, 255, 255], Opacity::<u8>::ONE),
        Coverage::fill(triangle),
    );
    let image = Srgba8Image::new(1, 1, vec![255; 4]).unwrap();
    let masked = Canvas::draw(content, Coverage::from_alpha(ImagePattern::new(image)));

    assert!(matches!(
        masked.node(),
        CanvasNode::Draw(Draw {
            source: ColorField::Canvas(_),
            coverage: Coverage {
                node: CoverageNode::AlphaOf(ColorField::Image(_))
            },
            ..
        })
    ));
}

#[test]
fn draw_keeps_source_and_coverage_as_distinct_semantics() {
    let source = ColorField::Solid(
        Color::new(Srgb, [0.8_f32, 0.2, 0.1], Opacity::<f32>::new(0.4).unwrap()).into(),
    );
    let coverage = Coverage::fill(Path::polygon([
        Point::new(0.0, 0.0),
        Point::new(1.0, 0.0),
        Point::new(0.0, 1.0),
    ]));
    let canvas = Canvas::draw(source.clone(), coverage.clone());

    assert!(matches!(
        canvas.node(),
        CanvasNode::Draw(Draw {
            source: ColorField::Solid(_),
            coverage: Coverage {
                node: CoverageNode::Fill { .. }
            },
        })
    ));
}

#[test]
fn destination_over_keeps_source_and_backdrop_roles_in_the_ir() {
    let backdrop = Canvas::draw(
        Color::new(Srgb, [10_u8, 20, 30], Opacity::<u8>::ONE),
        Coverage::all(),
    );
    let source = Canvas::draw(
        Color::new(Srgb, [40_u8, 50, 60], Opacity::<u8>::ONE),
        Coverage::all(),
    );
    let composition = Composition::new(Composite::DestinationOver, LinearSrgb)
        .with_blend(BlendMode::Screen, Srgb);
    let canvas = backdrop
        .clone()
        .composite(source.clone(), composition.clone());

    assert!(matches!(canvas.node(), CanvasNode::Composite {
        backdrop: recorded_backdrop,
        source: recorded_source,
        composition: recorded_composition,
    } if Arc::ptr_eq(&recorded_backdrop.node, &backdrop.node)
        && Arc::ptr_eq(&recorded_source.node, &source.node)
        && *recorded_composition == composition));
}

#[test]
fn transformed_wraps_the_canvas_without_baking_primitive_coordinates() {
    let triangle = [
        Point::new(0.0, 0.0),
        Point::new(1.0, 0.0),
        Point::new(0.0, 1.0),
    ];
    let transform = Projective2::from_rows([[2.0, 0.0, 10.0], [0.0, 3.0, 20.0], [0.0, 0.0, 1.0]]);
    let content = filled_triangle(
        triangle,
        ColorField::Solid(Color::new(Srgb, [255_u8, 255, 255], Opacity::<u8>::ONE).into()),
    );
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
        Color::new(Srgb, [255_u8, 255, 255], Opacity::<u8>::ONE),
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
    let view = CanvasView::axis_aligned(Point::new(0.0, 100.0), Point::new(200.0, 0.0)).unwrap();

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
        GradientStop::new(0.0, Color::new(Srgb, [255_u8, 0, 0], Opacity::<u8>::ONE)).unwrap(),
        GradientStop::new(0.4, Color::new(Srgb, [0_u8, 255, 0], Opacity::<u8>::ONE)).unwrap(),
        GradientStop::new(1.0, Color::new(Srgb, [0_u8, 0, 255], Opacity::<u8>::ONE)).unwrap(),
    ];
    let gradient = LinearGradient::new(
        Point::new(10.0, 20.0),
        Point::new(110.0, 20.0),
        stops.clone(),
        LinearSrgb,
    )
    .unwrap();

    assert_eq!(gradient.start(), Point::new(10.0, 20.0));
    assert_eq!(gradient.end(), Point::new(110.0, 20.0));
    assert_eq!(gradient.stops(), stops);
    assert_eq!(
        gradient.interpolation_space(),
        &crate::color::ColorSpace::from(LinearSrgb)
    );
}

#[test]
fn along_path_gradient_keeps_a_path_and_reusable_color_ramp() {
    let path = Path::polyline([
        Point::new(0.0, 0.0),
        Point::new(10.0, 0.0),
        Point::new(10.0, 10.0),
    ]);
    let ramp = ColorRamp::new(
        [
            GradientStop::new(0.0, Color::new(Srgb, [255_u8, 0, 0], Opacity::<u8>::ONE)).unwrap(),
            GradientStop::new(1.0, Color::new(Srgb, [0_u8, 0, 255], Opacity::<u8>::ONE)).unwrap(),
        ],
        LinearSrgb,
    )
    .unwrap();
    let gradient = AlongPathGradient::new(path.clone(), ramp.clone());

    assert_eq!(gradient.path(), &path);
    assert_eq!(gradient.ramp(), &ramp);
    assert!(matches!(
        ColorField::from(gradient),
        ColorField::AlongPathGradient(_)
    ));
}

#[test]
fn across_path_gradient_keeps_explicit_signed_distance_bounds() {
    let path = Path::polyline([Point::new(0.0, 0.0), Point::new(10.0, 0.0)]);
    let ramp = ColorRamp::new(
        [
            GradientStop::new(0.0, Color::new(Srgb, [255_u8, 0, 0], Opacity::<u8>::ONE)).unwrap(),
            GradientStop::new(1.0, Color::new(Srgb, [0_u8, 0, 255], Opacity::<u8>::ONE)).unwrap(),
        ],
        LinearSrgb,
    )
    .unwrap();
    let gradient = AcrossPathGradient::new(path.clone(), -2.0, 3.0, ramp.clone()).unwrap();

    assert_eq!(gradient.path(), &path);
    assert_eq!(gradient.from_distance(), -2.0);
    assert_eq!(gradient.to_distance(), 3.0);
    assert_eq!(gradient.ramp(), &ramp);
    assert!(matches!(
        ColorField::from(gradient),
        ColorField::AcrossPathGradient(_)
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
    let red = GradientStop::new(0.8, Color::new(Srgb, [255_u8, 0, 0], Opacity::<u8>::ONE)).unwrap();
    let blue =
        GradientStop::new(0.2, Color::new(Srgb, [0_u8, 0, 255], Opacity::<u8>::ONE)).unwrap();

    assert_eq!(
        ColorRamp::new([red.clone()], LinearSrgb),
        Err(ColorRampError::TooFewStops)
    );
    assert_eq!(
        ColorRamp::new([red, blue], LinearSrgb),
        Err(ColorRampError::UnorderedStops)
    );
}

#[test]
fn rejects_invalid_linear_gradient_structure_without_panicking() {
    let red = GradientStop::new(0.8, Color::new(Srgb, [255_u8, 0, 0], Opacity::<u8>::ONE)).unwrap();
    let blue =
        GradientStop::new(0.2, Color::new(Srgb, [0_u8, 0, 255], Opacity::<u8>::ONE)).unwrap();
    assert_eq!(
        GradientStop::new(f32::NAN, Color::new(Srgb, [0_u8, 0, 0], Opacity::<u8>::ONE)),
        Err(GradientStopError::NonFiniteOffset)
    );
    assert_eq!(
        GradientStop::new(1.1, Color::new(Srgb, [0_u8, 0, 0], Opacity::<u8>::ONE)),
        Err(GradientStopError::OffsetOutOfRange)
    );
    assert_eq!(
        LinearGradient::new(
            Point::new(0.0, 0.0),
            Point::new(0.0, 0.0),
            [red.clone(), blue.clone()],
            LinearSrgb,
        ),
        Err(LinearGradientError::CoincidentPoints)
    );
    assert_eq!(
        LinearGradient::new(
            Point::new(0.0, 0.0),
            Point::new(1.0, 0.0),
            [red, blue],
            LinearSrgb,
        ),
        Err(LinearGradientError::UnorderedStops)
    );
}

#[test]
fn mesh_gradient_records_a_colored_triangle_patch() {
    let points = [
        GradientPoint::new(
            Point::new(0.0, 0.0),
            Color::new(Srgb, [255_u8, 0, 0], Opacity::<u8>::ONE),
        ),
        GradientPoint::new(
            Point::new(4.0, 0.0),
            Color::new(Srgb, [0_u8, 255, 0], Opacity::<u8>::ONE),
        ),
        GradientPoint::new(
            Point::new(0.0, 4.0),
            Color::new(Srgb, [0_u8, 0, 255], Opacity::<u8>::ONE),
        ),
    ];
    let gradient = MeshGradient::from_triangle(points.clone(), LinearSrgb).unwrap();

    assert_eq!(gradient.triangles(), &[points]);
    assert_eq!(
        gradient.interpolation_space(),
        &crate::color::ColorSpace::from(LinearSrgb)
    );
}

#[test]
fn mesh_gradient_rejects_invalid_triangle_geometry() {
    let color = Color::new(Srgb, [255_u8, 255, 255], Opacity::<u8>::ONE);
    assert_eq!(
        MeshGradient::from_triangle(
            [
                GradientPoint::new(Point::new(0.0, 0.0), &color),
                GradientPoint::new(Point::new(1.0, 1.0), &color),
                GradientPoint::new(Point::new(2.0, 2.0), &color),
            ],
            LinearSrgb
        ),
        Err(MeshGradientError::DegenerateTriangle)
    );
    assert_eq!(
        MeshGradient::from_triangle(
            [
                GradientPoint::new(Point::new(f32::NAN, 0.0), &color),
                GradientPoint::new(Point::new(1.0, 0.0), &color),
                GradientPoint::new(Point::new(0.0, 1.0), &color),
            ],
            LinearSrgb
        ),
        Err(MeshGradientError::NonFinitePoint)
    );
}

#[test]
fn radial_and_conic_gradients_keep_their_canvas_coordinates() {
    let stops = [
        GradientStop::new(
            0.0,
            Color::new(Srgb, [255_u8, 255, 255], Opacity::<u8>::ONE),
        )
        .unwrap(),
        GradientStop::new(1.0, Color::new(Srgb, [0_u8, 0, 0], Opacity::<u8>::ONE)).unwrap(),
    ];
    let center = Point::new(20.0, 30.0);
    let radial = RadialGradient::new(center, 40.0, stops.clone(), LinearSrgb).unwrap();
    let conic = ConicGradient::new(center, 0.75, stops.clone(), LinearSrgb).unwrap();

    assert_eq!(radial.center(), center);
    assert_eq!(radial.radius(), 40.0);
    assert_eq!(radial.stops(), stops);
    assert_eq!(conic.center(), center);
    assert_eq!(conic.start_angle(), 0.75);
    assert_eq!(conic.stops(), stops);
}

#[test]
fn gradients_retain_extended_color_line_geometry() {
    let stops = [
        GradientStop::new(
            0.0,
            Color::new(Srgb, [255_u8, 255, 255], Opacity::<u8>::ONE),
        )
        .unwrap(),
        GradientStop::new(1.0, Color::new(Srgb, [0_u8, 0, 0], Opacity::<u8>::ONE)).unwrap(),
    ];
    let radial = RadialGradient::between_circles(
        Point::new(1.0, 2.0),
        3.0,
        Point::new(8.0, 9.0),
        12.0,
        stops.clone(),
        LinearSrgb,
    )
    .unwrap()
    .with_extend(GradientExtend::Reflect);
    let conic = ConicGradient::sweeping(Point::new(4.0, 5.0), -0.5, 1.25, stops, LinearSrgb)
        .unwrap()
        .with_extend(GradientExtend::Repeat);

    assert_eq!(radial.start_center(), Point::new(1.0, 2.0));
    assert_eq!(radial.start_radius(), 3.0);
    assert_eq!(radial.end_center(), Point::new(8.0, 9.0));
    assert_eq!(radial.end_radius(), 12.0);
    assert_eq!(radial.extend(), GradientExtend::Reflect);
    assert_eq!(conic.end_angle(), 1.25);
    assert_eq!(conic.extend(), GradientExtend::Repeat);
}

#[test]
fn two_circle_radial_gradient_preserves_signed_radii() {
    let stops = [
        GradientStop::new(
            0.0,
            Color::new(Srgb, [255_u8, 255, 255], Opacity::<u8>::ONE),
        )
        .unwrap(),
        GradientStop::new(1.0, Color::new(Srgb, [0_u8, 0, 0], Opacity::<u8>::ONE)).unwrap(),
    ];
    let gradient = RadialGradient::between_circles(
        Point::new(1.0, 2.0),
        -3.0,
        Point::new(8.0, 9.0),
        12.0,
        stops,
        LinearSrgb,
    )
    .unwrap();

    assert_eq!(gradient.start_radius(), -3.0);
    assert_eq!(gradient.end_radius(), 12.0);
}

#[test]
fn degenerate_gradients_remain_representable() {
    let stops = [
        GradientStop::new(
            0.0,
            Color::new(Srgb, [255_u8, 255, 255], Opacity::<u8>::ONE),
        )
        .unwrap(),
        GradientStop::new(1.0, Color::new(Srgb, [0_u8, 0, 0], Opacity::<u8>::ONE)).unwrap(),
    ];
    let circle = Point::new(1.0, 2.0);

    let radial =
        RadialGradient::between_circles(circle, 3.0, circle, 3.0, stops.clone(), LinearSrgb)
            .unwrap();
    let conic = ConicGradient::sweeping(circle, 0.5, 0.5, stops, LinearSrgb).unwrap();

    assert_eq!(radial.start_center(), radial.end_center());
    assert_eq!(radial.start_radius(), radial.end_radius());
    assert_eq!(conic.start_angle(), conic.end_angle());
}

#[test]
fn radial_and_conic_gradients_reject_invalid_geometry() {
    let stops = [
        GradientStop::new(
            0.0,
            Color::new(Srgb, [255_u8, 255, 255], Opacity::<u8>::ONE),
        )
        .unwrap(),
        GradientStop::new(1.0, Color::new(Srgb, [0_u8, 0, 0], Opacity::<u8>::ONE)).unwrap(),
    ];
    assert_eq!(
        RadialGradient::new(Point::new(0.0, 0.0), 0.0, stops.clone(), LinearSrgb),
        Err(RadialGradientError::NonPositiveRadius)
    );
    assert_eq!(
        RadialGradient::new(Point::new(0.0, 0.0), f32::NAN, stops.clone(), LinearSrgb),
        Err(RadialGradientError::NonFiniteRadius)
    );
    assert_eq!(
        ConicGradient::new(Point::new(0.0, 0.0), f32::INFINITY, stops, LinearSrgb),
        Err(ConicGradientError::NonFiniteStartAngle)
    );
}
