use super::color_space::PreparedColors;
use crate::canvas::{
    BlendShader, Canvas, CanvasIccColorError, CanvasNode, ColorField, Composite, CoverageNode,
    Draw, image::Image,
};
use crate::color::RgbColorSpace;

/// Resolves fallible authored colors before either backend starts recording.
/// The converted values belong to this render, not to the Canvas definition.
#[cfg(test)]
pub(crate) fn prepare_colors(canvas: &Canvas) -> Result<PreparedColors, CanvasIccColorError> {
    prepare_colors_in(canvas, RgbColorSpace::LinearSrgb)
}

pub(crate) fn prepare_colors_in(
    canvas: &Canvas,
    working_space: RgbColorSpace,
) -> Result<PreparedColors, CanvasIccColorError> {
    let mut prepared = PreparedColors::new(working_space);
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
            for stop in gradient.stops() {
                prepared.prepare(stop.color())?;
            }
        }
        ColorField::RadialGradient(gradient) => {
            for stop in gradient.stops() {
                prepared.prepare(stop.color())?;
            }
        }
        ColorField::ConicGradient(gradient) => {
            for stop in gradient.stops() {
                prepared.prepare(stop.color())?;
            }
        }
        ColorField::AlongPathGradient(gradient) => {
            for stop in gradient.ramp().stops() {
                prepared.prepare(stop.color())?;
            }
        }
        ColorField::AcrossPathGradient(gradient) => {
            for stop in gradient.ramp().stops() {
                prepared.prepare(stop.color())?;
            }
        }
        ColorField::MeshGradient(gradient) => {
            for triangle in gradient.triangles() {
                for point in triangle {
                    prepared.prepare(point.color())?;
                }
            }
        }
        ColorField::Canvas(canvas) => prepare_canvas_colors(canvas, prepared)?,
        ColorField::Image(_) | ColorField::Transparent => {}
    }
    Ok(())
}

/// Collects the blend programs needed to record this Canvas. Canvas images are
/// materialized separately and therefore run their own collection pass.
#[cfg(test)]
pub(crate) fn collect_blend_shaders(canvas: &Canvas, shaders: &mut Vec<BlendShader>) {
    collect_blend_shaders_in(canvas, shaders, RgbColorSpace::LinearSrgb);
}

pub(crate) fn collect_blend_shaders_in(
    canvas: &Canvas,
    shaders: &mut Vec<BlendShader>,
    working_space: RgbColorSpace,
) {
    match canvas.node() {
        CanvasNode::Draw(draw) => {
            collect_color_field_blend_shaders(&draw.source, shaders, working_space);
            if let CoverageNode::AlphaOf(source) = draw.coverage.node() {
                collect_color_field_blend_shaders(source, shaders, working_space);
            }
        }
        CanvasNode::Transform { content, .. } => {
            collect_blend_shaders_in(content, shaders, working_space)
        }
        CanvasNode::Composite {
            backdrop,
            source,
            composition,
        } => {
            if composition.requires_destination_read_in(working_space) {
                let shader = composition.blend_shader();
                if !shaders.contains(shader) {
                    shaders.push(shader.clone());
                }
            }
            collect_blend_shaders_in(backdrop, shaders, working_space);
            collect_blend_shaders_in(source, shaders, working_space);
        }
    }
}

fn collect_color_field_blend_shaders(
    source: &ColorField,
    shaders: &mut Vec<BlendShader>,
    working_space: RgbColorSpace,
) {
    if let ColorField::Canvas(canvas) = source {
        collect_blend_shaders_in(canvas, shaders, working_space);
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
        ColorField::Transparent => {}
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
    required_scratch_images_in(canvas, RgbColorSpace::LinearSrgb)
}

pub(crate) fn required_scratch_images_in(canvas: &Canvas, working_space: RgbColorSpace) -> usize {
    1 + additional_scratch_images(canvas, working_space)
}

fn additional_scratch_images(canvas: &Canvas, working_space: RgbColorSpace) -> usize {
    match canvas.node() {
        CanvasNode::Draw(draw) => match &draw.source {
            ColorField::Canvas(content) => {
                required_scratch_images_in(content, working_space)
                    + match draw.coverage.node() {
                        CoverageNode::All => 0,
                        CoverageNode::Fill { .. } | CoverageNode::Stroke { .. } => 1,
                        CoverageNode::AlphaOf(source) => {
                            required_color_field_scratch_images(source, working_space)
                        }
                    }
            }
            _ => match draw.coverage.node() {
                CoverageNode::AlphaOf(source) => {
                    1 + required_color_field_scratch_images(source, working_space)
                }
                CoverageNode::All | CoverageNode::Fill { .. } | CoverageNode::Stroke { .. } => 0,
            },
        },
        CanvasNode::Transform { content, .. } => additional_scratch_images(content, working_space),
        CanvasNode::Composite {
            backdrop,
            source,
            composition,
        } => {
            additional_scratch_images(backdrop, working_space)
                + if composition.blend().is_none()
                    && composition.composite() == Composite::SourceOver
                    && composition.composite_space() == working_space
                    && can_record_directly_in(source, working_space)
                {
                    additional_scratch_images(source, working_space)
                } else {
                    required_scratch_images_in(source, working_space)
                        + usize::from(composition.requires_destination_read_in(working_space))
                }
        }
    }
}

#[cfg(test)]
pub(crate) fn can_record_directly(canvas: &Canvas) -> bool {
    can_record_directly_in(canvas, RgbColorSpace::LinearSrgb)
}

pub(crate) fn can_record_directly_in(canvas: &Canvas, working_space: RgbColorSpace) -> bool {
    match canvas.node() {
        CanvasNode::Draw(_) => true,
        CanvasNode::Transform { content, .. } => can_record_directly_in(content, working_space),
        CanvasNode::Composite {
            backdrop,
            source,
            composition,
        } if composition.blend().is_none()
            && composition.composite() == Composite::SourceOver
            && composition.composite_space() == working_space =>
        {
            can_record_directly_in(backdrop, working_space)
                && can_record_directly_in(source, working_space)
        }
        CanvasNode::Composite { .. } => false,
    }
}

fn required_color_field_scratch_images(source: &ColorField, working_space: RgbColorSpace) -> usize {
    match source {
        ColorField::Canvas(canvas) => required_scratch_images_in(canvas, working_space),
        _ => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        canvas::{Blend, BlendMode, Composition, Coverage},
        color::{Color, LinearSrgb, Srgb},
    };

    #[test]
    fn destination_reading_compositions_collect_their_shader() {
        let leaf = || Canvas::draw(Color::rgb8(64, 128, 255), Coverage::all());
        let nonlinear = leaf().composite(leaf(), Composition::new(Composite::SourceOver, Srgb));
        let blended = leaf().composite(
            leaf(),
            Composition::new(Composite::SourceOver, LinearSrgb)
                .with_blend(BlendMode::Multiply, Srgb),
        );
        let canvas =
            nonlinear.composite(blended, Composition::new(Composite::SourceOver, LinearSrgb));

        let mut shaders = Vec::new();
        collect_blend_shaders(&canvas, &mut shaders);
        assert_eq!(shaders.len(), 2);
        assert!(shaders.contains(BlendShader::identity()));
        assert!(shaders.contains(Blend::new(BlendMode::Multiply, Srgb).shader()));
    }

    #[test]
    fn renderer_prepares_icc_colors_before_recording() {
        let profile =
            crate::color::IccProfile::from_bytes(include_bytes!("../color/fixtures/sRGB-v4.icc"))
                .unwrap();
        let color = crate::canvas::CanvasColor::from_icc(
            &profile,
            vec![0.25, 0.5, 0.75],
            crate::color::UnitF32::ONE,
            crate::color::IccRenderingIntent::RelativeColorimetric,
        )
        .unwrap();
        let canvas = Canvas::draw(color.clone(), Coverage::all());
        let prepared = prepare_colors(&canvas).unwrap();
        assert!(prepared.lower(&color)[0] > 0.0);
    }

    #[test]
    fn renderer_prepares_solid_colors_without_changing_canvas_storage() {
        let color = crate::canvas::CanvasColor::from(Color::rgb8(255, 128, 0));
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
        let leaf = || Canvas::draw(Color::rgb8(64, 128, 192), Coverage::all());
        let canvas = leaf().composite(
            leaf(),
            Composition::new(Composite::SourceOver, crate::color::LinearDisplayP3),
        );
        assert!(!can_record_directly_in(&canvas, RgbColorSpace::LinearSrgb));
        assert_eq!(
            required_scratch_images_in(&canvas, RgbColorSpace::LinearSrgb),
            3
        );
        assert!(can_record_directly_in(
            &canvas,
            RgbColorSpace::LinearDisplayP3
        ));
        assert_eq!(
            required_scratch_images_in(&canvas, RgbColorSpace::LinearDisplayP3),
            1
        );
    }
}
