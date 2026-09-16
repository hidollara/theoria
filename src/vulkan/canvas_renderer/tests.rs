use super::super::image::GpuAlphaMode;
use super::canvas_plan::{can_record_directly, collect_images, required_scratch_images};
use super::encoding::ImagePushConstants;
use super::pipeline::composite_blend_factors;
use super::*;
use crate::canvas::{
    Composite, Coverage, ImagePattern, canvas_image::CanvasImage, image::ImageKind,
};
use crate::color::Color;
use crate::geometry::Point;
use crate::image::Srgba8Image;
use crate::transform::Projective2;

#[test]
fn canvas_image_is_an_image_boundary_while_canvas_color_field_stays_semantic() {
    let pixels = Srgba8Image::new(1, 1, vec![255; 4]).unwrap();
    let producer = Canvas::draw(pixels.clone(), Coverage::all());
    let view = CanvasView::axis_aligned(Point::new(0.0, 0.0), Point::new(1.0, 1.0)).unwrap();
    let image = CanvasImage::new(producer.clone(), view, 1, 1).unwrap();

    let mut outer_images = Vec::new();
    collect_images(
        &Canvas::draw(image.clone(), Coverage::all()),
        &mut outer_images,
    );
    assert_eq!(outer_images.len(), 1);
    assert!(matches!(outer_images[0].kind(),
        ImageKind::Canvas(source) if source.instance_key() == image.instance_key()
    ));

    let mut canvas_dependencies = Vec::new();
    collect_images(
        &Canvas::draw(producer, Coverage::all()),
        &mut canvas_dependencies,
    );
    assert_eq!(canvas_dependencies.len(), 1);
    assert!(matches!(
        canvas_dependencies[0].kind(),
        ImageKind::Srgba8(source) if source.instance_key() == pixels.instance_key()
    ));
}

#[test]
fn image_sampling_respects_the_gpu_images_alpha_representation() {
    let straight = Srgba8Image::new(1, 1, vec![255, 128, 64, 128]).unwrap();
    let canvas_image = CanvasImage::new(
        Canvas::empty(),
        CanvasView::axis_aligned(Point::new(0.0, 0.0), Point::new(1.0, 1.0)).unwrap(),
        1,
        1,
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
        &ImagePattern::new(canvas_image),
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
