use super::color_space::PreparedColors;
use crate::canvas::{
    Canvas, CanvasIccColorError, CanvasNode, ColorField, Composite, Composition, CoverageNode,
    Draw, image::Image,
};
use crate::color::ColorSpace;
#[cfg(test)]
use crate::color::LinearSrgb;

/// Resolves fallible authored colors before either backend starts recording.
/// The converted values belong to this render, not to the Canvas definition.
#[cfg(test)]
pub(crate) fn prepare_colors(canvas: &Canvas) -> Result<PreparedColors, CanvasIccColorError> {
    prepare_colors_in(canvas, LinearSrgb)
}

pub(crate) fn prepare_colors_in(
    canvas: &Canvas,
    working_profile: impl Into<ColorSpace>,
) -> Result<PreparedColors, CanvasIccColorError> {
    let mut prepared = PreparedColors::new(working_profile);
    prepare_canvas_colors(canvas, &mut prepared)?;
    Ok(prepared)
}

fn prepare_canvas_colors(
    canvas: &Canvas,
    prepared: &mut PreparedColors,
) -> Result<(), CanvasIccColorError> {
    match canvas.node() {
        CanvasNode::Draw(draw) => {
            prepare_color_field(&draw.source, prepared)?;
            if let CoverageNode::AlphaOf(source) = draw.coverage.node() {
                prepare_color_field(source, prepared)?;
            }
        }
        CanvasNode::Transform { content, .. } => prepare_canvas_colors(content, prepared)?,
        CanvasNode::Composite {
            backdrop, source, ..
        } => {
            prepare_canvas_colors(backdrop, prepared)?;
            prepare_canvas_colors(source, prepared)?;
        }
    }
    Ok(())
}

fn prepare_color_field(
    source: &ColorField,
    prepared: &mut PreparedColors,
) -> Result<(), CanvasIccColorError> {
    match source {
        ColorField::Solid(color) => prepared.prepare(color)?,
        ColorField::LinearGradient(gradient) => {
            prepared.prepare_interpolation(gradient.interpolation_space())?;
            for stop in gradient.stops() {
                prepared.prepare(stop.color())?;
            }
        }
        ColorField::RadialGradient(gradient) => {
            prepared.prepare_interpolation(gradient.interpolation_space())?;
            for stop in gradient.stops() {
                prepared.prepare(stop.color())?;
            }
        }
        ColorField::ConicGradient(gradient) => {
            prepared.prepare_interpolation(gradient.interpolation_space())?;
            for stop in gradient.stops() {
                prepared.prepare(stop.color())?;
            }
        }
        ColorField::AlongPathGradient(gradient) => {
            prepared.prepare_interpolation(gradient.ramp().interpolation_space())?;
            for stop in gradient.ramp().stops() {
                prepared.prepare(stop.color())?;
            }
        }
        ColorField::AcrossPathGradient(gradient) => {
            prepared.prepare_interpolation(gradient.ramp().interpolation_space())?;
            for stop in gradient.ramp().stops() {
                prepared.prepare(stop.color())?;
            }
        }
        ColorField::MeshGradient(gradient) => {
            prepared.prepare_interpolation(gradient.interpolation_space())?;
            for triangle in gradient.triangles() {
                for point in triangle {
                    prepared.prepare(point.color())?;
                }
            }
        }
        ColorField::Canvas(canvas) => prepare_canvas_colors(canvas, prepared)?,
        ColorField::Image(_) => {}
    }
    Ok(())
}

/// Collects the blend programs needed to record this Canvas. Canvas images are
/// materialized separately and therefore run their own collection pass.
#[cfg(test)]
pub(crate) fn collect_compositions(canvas: &Canvas, compositions: &mut Vec<Composition>) {
    collect_compositions_in(canvas, compositions, &ColorSpace::from(LinearSrgb));
}

pub(crate) fn collect_compositions_in(
    canvas: &Canvas,
    compositions: &mut Vec<Composition>,
    working_profile: &ColorSpace,
) {
    match canvas.node() {
        CanvasNode::Draw(draw) => {
            collect_color_field_compositions(&draw.source, compositions, working_profile);
            if let CoverageNode::AlphaOf(source) = draw.coverage.node() {
                collect_color_field_compositions(source, compositions, working_profile);
            }
        }
        CanvasNode::Transform { content, .. } => {
            collect_compositions_in(content, compositions, working_profile)
        }
        CanvasNode::Composite {
            backdrop,
            source,
            composition,
        } => {
            if composition.requires_destination_read_in(working_profile)
                && !compositions.contains(composition)
            {
                compositions.push(composition.clone());
            }
            collect_compositions_in(backdrop, compositions, working_profile);
            collect_compositions_in(source, compositions, working_profile);
        }
    }
}

fn collect_color_field_compositions(
    source: &ColorField,
    compositions: &mut Vec<Composition>,
    working_profile: &ColorSpace,
) {
    if let ColorField::Canvas(canvas) = source {
        collect_compositions_in(canvas, compositions, working_profile);
    }
}

pub(crate) fn collect_images(canvas: &Canvas, images: &mut Vec<Image>) {
    match canvas.node() {
        CanvasNode::Draw(draw) => collect_draw_images(draw, images),
        CanvasNode::Transform { content, .. } => collect_images(content, images),
        CanvasNode::Composite {
            backdrop, source, ..
        } => {
            collect_images(backdrop, images);
            collect_images(source, images);
        }
    }
}

fn collect_draw_images(draw: &Draw, images: &mut Vec<Image>) {
    collect_color_field_images(&draw.source, images);
    if let CoverageNode::AlphaOf(source) = draw.coverage.node() {
        collect_color_field_images(source, images);
    }
}

fn collect_color_field_images(source: &ColorField, images: &mut Vec<Image>) {
    match source {
        ColorField::Image(pattern) => images.push(pattern.image().clone()),
        ColorField::Canvas(canvas) => collect_images(canvas, images),
        ColorField::Solid(_)
        | ColorField::LinearGradient(_)
        | ColorField::RadialGradient(_)
        | ColorField::ConicGradient(_)
        | ColorField::AlongPathGradient(_)
        | ColorField::AcrossPathGradient(_)
        | ColorField::MeshGradient(_) => {}
    }
}

#[cfg(test)]
pub(crate) fn required_scratch_images(canvas: &Canvas) -> usize {
    required_scratch_images_in(canvas, &ColorSpace::from(LinearSrgb))
}

pub(crate) fn required_scratch_images_in(canvas: &Canvas, working_profile: &ColorSpace) -> usize {
    1 + additional_scratch_images(canvas, working_profile)
}

fn additional_scratch_images(canvas: &Canvas, working_profile: &ColorSpace) -> usize {
    match canvas.node() {
        CanvasNode::Draw(draw) => match &draw.source {
            ColorField::Canvas(content) => {
                required_scratch_images_in(content, working_profile)
                    + match draw.coverage.node() {
                        CoverageNode::All => 0,
                        CoverageNode::Fill { .. } | CoverageNode::Stroke { .. } => 1,
                        CoverageNode::AlphaOf(source) => {
                            required_color_field_scratch_images(source, working_profile)
                        }
                    }
            }
            _ => match draw.coverage.node() {
                CoverageNode::AlphaOf(source) => {
                    1 + required_color_field_scratch_images(source, working_profile)
                }
                CoverageNode::All | CoverageNode::Fill { .. } | CoverageNode::Stroke { .. } => 0,
            },
        },
        CanvasNode::Transform { content, .. } => {
            additional_scratch_images(content, working_profile)
        }
        CanvasNode::Composite {
            backdrop,
            source,
            composition,
        } => {
            additional_scratch_images(backdrop, working_profile)
                + if composition.blend().is_none()
                    && composition.composite() == Composite::SourceOver
                    && composition.composite_space() == working_profile
                    && can_record_directly_in(source, working_profile)
                {
                    additional_scratch_images(source, working_profile)
                } else {
                    required_scratch_images_in(source, working_profile)
                        + usize::from(composition.requires_destination_read_in(working_profile))
                }
        }
    }
}

#[cfg(test)]
pub(crate) fn can_record_directly(canvas: &Canvas) -> bool {
    can_record_directly_in(canvas, &ColorSpace::from(LinearSrgb))
}

pub(crate) fn can_record_directly_in(canvas: &Canvas, working_profile: &ColorSpace) -> bool {
    match canvas.node() {
        CanvasNode::Draw(_) => true,
        CanvasNode::Transform { content, .. } => can_record_directly_in(content, working_profile),
        CanvasNode::Composite {
            backdrop,
            source,
            composition,
        } if composition.blend().is_none()
            && composition.composite() == Composite::SourceOver
            && composition.composite_space() == working_profile =>
        {
            can_record_directly_in(backdrop, working_profile)
                && can_record_directly_in(source, working_profile)
        }
        CanvasNode::Composite { .. } => false,
    }
}

fn required_color_field_scratch_images(source: &ColorField, working_profile: &ColorSpace) -> usize {
    match source {
        ColorField::Canvas(canvas) => required_scratch_images_in(canvas, working_profile),
        _ => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        canvas::{BlendMode, Composition, Coverage},
        color::{Color, LinearSrgb, Opacity, Srgb},
    };

    #[test]
    fn destination_reading_compositions_collect_their_shader() {
        let leaf = || {
            Canvas::draw(
                Color::new(Srgb, [64_u8, 128, 255], Opacity::<u8>::ONE),
                Coverage::all(),
            )
        };
        let nonlinear = leaf().composite(leaf(), Composition::new(Composite::SourceOver, Srgb));
        let blended = leaf().composite(
            leaf(),
            Composition::new(Composite::SourceOver, LinearSrgb)
                .with_blend(BlendMode::Multiply, Srgb),
        );
        let canvas =
            nonlinear.composite(blended, Composition::new(Composite::SourceOver, LinearSrgb));

        let mut compositions = Vec::new();
        collect_compositions(&canvas, &mut compositions);
        assert_eq!(compositions.len(), 2);
        assert!(compositions.contains(&Composition::new(Composite::SourceOver, Srgb)));
        assert!(
            compositions.contains(
                &Composition::new(Composite::SourceOver, LinearSrgb)
                    .with_blend(BlendMode::Multiply, Srgb)
            )
        );
    }

    #[test]
    fn distinct_profile_handles_keep_distinct_composition_pipelines() {
        let leaf = || {
            Canvas::draw(
                Color::new(Srgb, [64_u8, 128, 255], Opacity::<u8>::ONE),
                Coverage::all(),
            )
        };
        let first = crate::test_support::distinct_color_profile(Srgb);
        let second = crate::test_support::distinct_color_profile(Srgb);
        let canvas = leaf()
            .composite(leaf(), Composition::new(Composite::SourceOver, &first))
            .composite(leaf(), Composition::new(Composite::SourceOver, &second));
        let mut compositions = Vec::new();
        collect_compositions(&canvas, &mut compositions);
        assert_eq!(compositions.len(), 2);
    }

    #[test]
    fn renderer_prepares_icc_colors_before_recording() {
        let profile = crate::color::IccProfile::from_bytes(include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/crates/theoria-color/src/color/fixtures/sRGB-v4.icc"
        )))
        .unwrap();
        let color = crate::canvas::CanvasColor::from_icc(
            &profile,
            vec![0.25, 0.5, 0.75],
            Opacity::<f32>::ONE,
            crate::color::IccRenderingIntent::RelativeColorimetric,
        )
        .unwrap();
        let canvas = Canvas::draw(color.clone(), Coverage::all());
        let prepared = prepare_colors(&canvas).unwrap();
        assert!(prepared.lower(&color)[0] > 0.0);
    }

    #[test]
    fn renderer_prepares_solid_colors_without_changing_canvas_storage() {
        let color = crate::canvas::CanvasColor::from(Color::new(
            Srgb,
            [255_u8, 128, 0],
            Opacity::<u8>::ONE,
        ));
        let canvas = Canvas::draw(color.clone(), Coverage::all());
        let prepared = prepare_colors(&canvas).unwrap();
        let CanvasNode::Draw(draw) = canvas.node() else {
            unreachable!()
        };
        let ColorField::Solid(stored) = &draw.source else {
            unreachable!()
        };

        assert_eq!(stored, &color);
        assert_eq!(prepared.lower(stored)[0], 1.0);
    }

    #[test]
    fn direct_composition_requires_the_same_working_space() {
        let leaf = || {
            Canvas::draw(
                Color::new(Srgb, [64_u8, 128, 192], Opacity::<u8>::ONE),
                Coverage::all(),
            )
        };
        let canvas = leaf().composite(
            leaf(),
            Composition::new(Composite::SourceOver, crate::color::LinearDisplayP3),
        );
        let linear_srgb = ColorSpace::from(crate::color::LinearSrgb);
        let linear_p3 = ColorSpace::from(crate::color::LinearDisplayP3);
        assert!(!can_record_directly_in(&canvas, &linear_srgb));
        assert_eq!(required_scratch_images_in(&canvas, &linear_srgb), 3);
        assert!(can_record_directly_in(&canvas, &linear_p3));
        assert_eq!(required_scratch_images_in(&canvas, &linear_p3), 1);
    }
}
