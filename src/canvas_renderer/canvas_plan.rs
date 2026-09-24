use crate::canvas::{
    BlendShader, Canvas, CanvasIccColorError, CanvasNode, ColorField, Composite, CoverageNode,
    Draw, image::Image,
};

/// Resolves fallible authored colors before either backend starts recording.
/// The prepared linear-sRGB values are shared with the Canvas's cloned colors.
pub(crate) fn prepare_colors(canvas: &Canvas) -> Result<(), CanvasIccColorError> {
    match canvas.node() {
        CanvasNode::Draw(draw) => {
            prepare_color_field(&draw.source)?;
            if let CoverageNode::AlphaOf(source) = draw.coverage.node() {
                prepare_color_field(source)?;
            }
        }
        CanvasNode::Transform { content, .. } => prepare_colors(content)?,
        CanvasNode::Composite {
            backdrop, source, ..
        } => {
            prepare_colors(backdrop)?;
            prepare_colors(source)?;
        }
    }
    Ok(())
}

fn prepare_color_field(source: &ColorField) -> Result<(), CanvasIccColorError> {
    match source {
        ColorField::Solid(color) => color.prepare_linear_srgb()?,
        ColorField::LinearGradient(gradient) => {
            for stop in gradient.stops() {
                stop.color().prepare_linear_srgb()?;
            }
        }
        ColorField::RadialGradient(gradient) => {
            for stop in gradient.stops() {
                stop.color().prepare_linear_srgb()?;
            }
        }
        ColorField::ConicGradient(gradient) => {
            for stop in gradient.stops() {
                stop.color().prepare_linear_srgb()?;
            }
        }
        ColorField::AlongPathGradient(gradient) => {
            for stop in gradient.ramp().stops() {
                stop.color().prepare_linear_srgb()?;
            }
        }
        ColorField::AcrossPathGradient(gradient) => {
            for stop in gradient.ramp().stops() {
                stop.color().prepare_linear_srgb()?;
            }
        }
        ColorField::MeshGradient(gradient) => {
            for triangle in gradient.triangles() {
                for point in triangle {
                    point.color().prepare_linear_srgb()?;
                }
            }
        }
        ColorField::Canvas(canvas) => prepare_colors(canvas)?,
        ColorField::Image(_) | ColorField::Transparent => {}
    }
    Ok(())
}

/// Collects the blend programs needed to record this Canvas. Canvas images are
/// materialized separately and therefore run their own collection pass.
pub(crate) fn collect_blend_shaders(canvas: &Canvas, shaders: &mut Vec<BlendShader>) {
    match canvas.node() {
        CanvasNode::Draw(draw) => {
            collect_color_field_blend_shaders(&draw.source, shaders);
            if let CoverageNode::AlphaOf(source) = draw.coverage.node() {
                collect_color_field_blend_shaders(source, shaders);
            }
        }
        CanvasNode::Transform { content, .. } => collect_blend_shaders(content, shaders),
        CanvasNode::Composite {
            backdrop,
            source,
            composition,
        } => {
            if composition.requires_destination_read() {
                let shader = composition.blend_shader();
                if !shaders.contains(shader) {
                    shaders.push(shader.clone());
                }
            }
            collect_blend_shaders(backdrop, shaders);
            collect_blend_shaders(source, shaders);
        }
    }
}

fn collect_color_field_blend_shaders(source: &ColorField, shaders: &mut Vec<BlendShader>) {
    if let ColorField::Canvas(canvas) = source {
        collect_blend_shaders(canvas, shaders);
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

pub(crate) fn required_scratch_images(canvas: &Canvas) -> usize {
    1 + additional_scratch_images(canvas)
}

fn additional_scratch_images(canvas: &Canvas) -> usize {
    match canvas.node() {
        CanvasNode::Draw(draw) => match &draw.source {
            ColorField::Canvas(content) => {
                required_scratch_images(content)
                    + match draw.coverage.node() {
                        CoverageNode::All => 0,
                        CoverageNode::Fill { .. } | CoverageNode::Stroke { .. } => 1,
                        CoverageNode::AlphaOf(source) => {
                            required_color_field_scratch_images(source)
                        }
                    }
            }
            _ => match draw.coverage.node() {
                CoverageNode::AlphaOf(source) => 1 + required_color_field_scratch_images(source),
                CoverageNode::All | CoverageNode::Fill { .. } | CoverageNode::Stroke { .. } => 0,
            },
        },
        CanvasNode::Transform { content, .. } => additional_scratch_images(content),
        CanvasNode::Composite {
            backdrop,
            source,
            composition,
        } => {
            additional_scratch_images(backdrop)
                + if composition.blend().is_none()
                    && composition.composite() == Composite::SourceOver
                    && composition.composite_space().is_linear()
                    && can_record_directly(source)
                {
                    additional_scratch_images(source)
                } else {
                    required_scratch_images(source)
                        + usize::from(composition.requires_destination_read())
                }
        }
    }
}

pub(crate) fn can_record_directly(canvas: &Canvas) -> bool {
    match canvas.node() {
        CanvasNode::Draw(_) => true,
        CanvasNode::Transform { content, .. } => can_record_directly(content),
        CanvasNode::Composite {
            backdrop,
            source,
            composition,
        } if composition.blend().is_none()
            && composition.composite() == Composite::SourceOver
            && composition.composite_space().is_linear() =>
        {
            can_record_directly(backdrop) && can_record_directly(source)
        }
        CanvasNode::Composite { .. } => false,
    }
}

fn required_color_field_scratch_images(source: &ColorField) -> usize {
    match source {
        ColorField::Canvas(canvas) => required_scratch_images(canvas),
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
        prepare_colors(&canvas).unwrap();
        assert!(color.straight_rgba_in(crate::color::RgbColorSpace::LinearSrgb)[0] > 0.0);
    }
}
