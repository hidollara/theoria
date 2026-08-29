use crate::{
    canvas::{BlendMode, Canvas, CanvasNode, ColorField, Composite, CoverageNode, Draw},
    image::Image,
};

pub(super) fn collect_images(canvas: &Canvas, images: &mut Vec<Image>) {
    match canvas.node() {
        CanvasNode::Empty => {}
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

pub(super) fn required_scratch_images(canvas: &Canvas) -> usize {
    1 + additional_scratch_images(canvas)
}

fn additional_scratch_images(canvas: &Canvas) -> usize {
    match canvas.node() {
        CanvasNode::Empty => 0,
        CanvasNode::Draw(draw) => {
            if (draw.blend_mode != BlendMode::Normal || draw.composite != Composite::SourceOver)
                && !matches!(draw.coverage.node(), CoverageNode::All)
            {
                // Full semantic lowering keeps the original destination, the
                // composite candidate, the isolated source, and the coverage
                // field alive until the final coverage interpolation.
                return required_color_field_scratch_images(&draw.source)
                    + required_coverage_scratch_images(&draw.coverage)
                    + 2;
            }
            let source_work = match &draw.source {
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
                    CoverageNode::AlphaOf(source) => {
                        1 + required_color_field_scratch_images(source)
                    }
                    CoverageNode::All | CoverageNode::Fill { .. } | CoverageNode::Stroke { .. } => {
                        0
                    }
                },
            };
            if draw.blend_mode == BlendMode::Normal && draw.composite == Composite::SourceOver {
                source_work
            } else {
                // A non-default draw is first evaluated as an isolated source.
                // Destination-reading blends additionally preserve the accumulator.
                source_work + 1 + usize::from(draw.blend_mode != BlendMode::Normal)
            }
        }
        CanvasNode::Transform { content, .. } => additional_scratch_images(content),
        CanvasNode::Composite {
            backdrop,
            source,
            blend_mode,
            composite,
        } => {
            additional_scratch_images(backdrop)
                + if *blend_mode == BlendMode::Normal
                    && *composite == Composite::SourceOver
                    && can_record_directly(source)
                {
                    additional_scratch_images(source)
                } else {
                    required_scratch_images(source) + usize::from(*blend_mode != BlendMode::Normal)
                }
        }
    }
}

fn required_coverage_scratch_images(coverage: &crate::canvas::Coverage) -> usize {
    match coverage.node() {
        CoverageNode::All => 0,
        CoverageNode::AlphaOf(source) => required_color_field_scratch_images(source),
        CoverageNode::Fill { .. } | CoverageNode::Stroke { .. } => 1,
    }
}

pub(super) fn can_record_directly(canvas: &Canvas) -> bool {
    match canvas.node() {
        CanvasNode::Empty => true,
        CanvasNode::Draw(draw) => {
            draw.blend_mode == BlendMode::Normal && draw.composite == Composite::SourceOver
        }
        CanvasNode::Transform { content, .. } => can_record_directly(content),
        CanvasNode::Composite {
            backdrop,
            source,
            blend_mode: BlendMode::Normal,
            composite: Composite::SourceOver,
        } => can_record_directly(backdrop) && can_record_directly(source),
        CanvasNode::Composite { .. } => false,
    }
}

fn required_color_field_scratch_images(source: &ColorField) -> usize {
    match source {
        ColorField::Canvas(canvas) => required_scratch_images(canvas),
        _ => 1,
    }
}
