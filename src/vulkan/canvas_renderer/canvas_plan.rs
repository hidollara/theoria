use crate::{
    canvas::{Canvas, CanvasNode, CompositeOperator, CoverageNode, Draw, Source},
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
    collect_source_images(&draw.source, images);
    if let CoverageNode::SourceAlpha(source) = draw.coverage.node() {
        collect_source_images(source, images);
    }
}

fn collect_source_images(source: &Source, images: &mut Vec<Image>) {
    match source {
        Source::Image(pattern) => images.push(pattern.image().clone()),
        Source::Canvas(canvas) => collect_images(canvas, images),
        Source::Solid(_)
        | Source::LinearGradient(_)
        | Source::RadialGradient(_)
        | Source::ConicGradient(_)
        | Source::AlongPathGradient(_)
        | Source::AcrossPathGradient(_)
        | Source::MeshGradient(_) => {}
    }
}

pub(super) fn required_scratch_images(canvas: &Canvas) -> usize {
    1 + additional_scratch_images(canvas)
}

fn additional_scratch_images(canvas: &Canvas) -> usize {
    match canvas.node() {
        CanvasNode::Empty => 0,
        CanvasNode::Draw(draw) => match &draw.source {
            Source::Canvas(content) => {
                required_scratch_images(content)
                    + match draw.coverage.node() {
                        CoverageNode::All => 0,
                        CoverageNode::Fill { .. } | CoverageNode::Stroke { .. } => 1,
                        CoverageNode::SourceAlpha(source) => required_source_scratch_images(source),
                    }
            }
            _ => match draw.coverage.node() {
                CoverageNode::SourceAlpha(source) => 1 + required_source_scratch_images(source),
                CoverageNode::All | CoverageNode::Fill { .. } | CoverageNode::Stroke { .. } => 0,
            },
        },
        CanvasNode::Transform { content, .. } => additional_scratch_images(content),
        CanvasNode::Composite {
            backdrop,
            source,
            operator,
        } => {
            additional_scratch_images(backdrop)
                + if *operator == CompositeOperator::SourceOver && can_record_directly(source) {
                    additional_scratch_images(source)
                } else {
                    required_scratch_images(source)
                }
        }
    }
}

pub(super) fn can_record_directly(canvas: &Canvas) -> bool {
    match canvas.node() {
        CanvasNode::Empty | CanvasNode::Draw(_) => true,
        CanvasNode::Transform { content, .. } => can_record_directly(content),
        CanvasNode::Composite {
            backdrop,
            source,
            operator: CompositeOperator::SourceOver,
        } => can_record_directly(backdrop) && can_record_directly(source),
        CanvasNode::Composite { .. } => false,
    }
}

fn required_source_scratch_images(source: &Source) -> usize {
    match source {
        Source::Canvas(canvas) => required_scratch_images(canvas),
        _ => 1,
    }
}
