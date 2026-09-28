use ash::vk;

mod commands;

use crate::{
    canvas::Color,
    canvas::{
        Canvas, CanvasNode, CanvasView, ColorField, Composite, Composition, Coverage, CoverageNode,
        Draw,
    },
    canvas_renderer::{canvas_plan::can_record_directly_in, color_space::PreparedColors},
    color::{ColorSpace, Opacity, Srgb},
    surface::OutputAlpha,
    tessellation::{Mesh, tessellate_fill_with_tolerance, tessellate_stroke_with_tolerance},
    transform::Projective2,
    vulkan::{color::PremultipliedWorkingRgba, image::ImageCache, scratch_image::ScratchImage},
};

use self::commands::CanvasCommandEncoder;
use super::pipeline::{Pipeline, Pipelines};

pub(super) struct CanvasRecorder<'a> {
    commands: CanvasCommandEncoder<'a>,
    next_image: usize,
    working_space: ColorSpace,
}

impl<'a> CanvasRecorder<'a> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        device: &'a ash::Device,
        command_buffer: vk::CommandBuffer,
        pipelines: &'a Pipelines,
        image_cache: &'a ImageCache,
        scratch_images: &'a [ScratchImage],
        attachment_area: vk::Rect2D,
        target_region: Option<(vk::Viewport, vk::Rect2D)>,
        view: CanvasView,
        prepared: &'a PreparedColors,
    ) -> Self {
        Self {
            commands: CanvasCommandEncoder::new(
                device,
                command_buffer,
                pipelines,
                image_cache,
                scratch_images,
                attachment_area,
                target_region,
                view,
                prepared,
            ),
            next_image: 0,
            working_space: prepared.working_space().clone(),
        }
    }

    pub(super) fn record(
        &mut self,
        canvas: &Canvas,
        image_view: vk::ImageView,
        clear_color: PremultipliedWorkingRgba,
        present_pipeline: &Pipeline,
        output_alpha: Option<OutputAlpha>,
    ) {
        let canvas_image = self.evaluate(canvas, Projective2::IDENTITY);
        self.commands.present(
            canvas_image,
            image_view,
            clear_color,
            present_pipeline,
            output_alpha,
        );
    }

    fn evaluate(&mut self, canvas: &Canvas, canvas_to_root: Projective2) -> usize {
        let accumulator = self.next_image;
        self.next_image += 1;
        self.commands
            .begin_accumulator(accumulator, vk::AttachmentLoadOp::CLEAR);
        self.render_node(accumulator, canvas, canvas_to_root);
        self.commands.end_rendering();
        self.commands.transition_to_sampled(accumulator);
        accumulator
    }

    fn render_node(&mut self, accumulator: usize, canvas: &Canvas, canvas_to_root: Projective2) {
        match canvas.node() {
            CanvasNode::Draw(draw) => {
                self.render_draw(accumulator, draw, canvas_to_root);
            }
            CanvasNode::Transform { content, transform } => {
                self.render_node(accumulator, content, transform.then(canvas_to_root));
            }
            CanvasNode::Composite {
                backdrop,
                source,
                composition,
            } => {
                self.render_node(accumulator, backdrop, canvas_to_root);
                if composition.blend().is_none()
                    && composition.composite() == Composite::SourceOver
                    && composition.composite_space() == &self.working_space
                    && can_record_directly_in(source, &self.working_space)
                {
                    self.render_node(accumulator, source, canvas_to_root);
                } else {
                    self.commands.end_rendering();
                    let source_image = self.evaluate(source, canvas_to_root);
                    if !composition.requires_destination_read_in(&self.working_space) {
                        self.commands.resume_accumulator(accumulator);
                        self.commands
                            .draw_composite(source_image, composition.composite());
                    } else {
                        self.compose_with_destination(accumulator, source_image, composition);
                    }
                }
            }
        }
    }

    fn render_draw(&mut self, accumulator: usize, draw: &Draw, draw_to_root: Projective2) {
        self.render_source_with_coverage(accumulator, draw, draw_to_root);
    }

    fn render_source_with_coverage(
        &mut self,
        accumulator: usize,
        draw: &Draw,
        draw_to_root: Projective2,
    ) {
        if !self.commands.set_canvas_region() {
            return;
        }

        if let ColorField::Canvas(content) = &draw.source {
            self.render_canvas_source(accumulator, content, &draw.coverage, draw_to_root);
            return;
        }

        match draw.coverage.node() {
            CoverageNode::All => self
                .commands
                .render_unbounded_color_field(&draw.source, draw_to_root),
            CoverageNode::Fill { path, rule } => {
                let tolerance = self.commands.path_tolerance(path, draw_to_root);
                let mesh = tessellate_fill_with_tolerance(path, *rule, tolerance);
                self.commands.render_mesh(&mesh, &draw.source, draw_to_root);
            }
            CoverageNode::Stroke { path, style } => {
                let tolerance = self.commands.path_tolerance(path, draw_to_root);
                let mesh = tessellate_stroke_with_tolerance(path, style, tolerance);
                self.commands.render_mesh(&mesh, &draw.source, draw_to_root);
            }
            CoverageNode::AlphaOf(coverage_source) => {
                self.commands.end_rendering();
                let content_image = self.evaluate_color_field(&draw.source, draw_to_root);
                let mask_image = self.evaluate_color_field(coverage_source, draw_to_root);
                self.commands.apply_mask(content_image, mask_image);
                self.commands.resume_accumulator(accumulator);
                self.commands
                    .draw_composite(content_image, Composite::SourceOver);
            }
        }
    }

    fn compose_with_destination(
        &mut self,
        accumulator: usize,
        source: usize,
        composition: &Composition,
    ) {
        self.commands.transition_to_sampled(accumulator);
        let destination = self.next_image;
        self.next_image += 1;
        self.commands
            .begin_accumulator(destination, vk::AttachmentLoadOp::CLEAR);
        self.commands.draw_composite(accumulator, Composite::Copy);
        self.commands.end_rendering();
        self.commands.transition_to_sampled(destination);

        self.commands
            .begin_accumulator(accumulator, vk::AttachmentLoadOp::CLEAR);
        self.commands
            .draw_composition(destination, source, composition);
    }

    fn evaluate_coverage(
        &mut self,
        coverage: &Coverage,
        coverage_to_root: Projective2,
    ) -> Option<usize> {
        match coverage.node() {
            CoverageNode::All => None,
            CoverageNode::AlphaOf(source) => {
                Some(self.evaluate_color_field(source, coverage_to_root))
            }
            CoverageNode::Fill { path, rule } => {
                let tolerance = self.commands.path_tolerance(path, coverage_to_root);
                let mesh = tessellate_fill_with_tolerance(path, *rule, tolerance);
                Some(self.evaluate_geometry_mask(&mesh, coverage_to_root))
            }
            CoverageNode::Stroke { path, style } => {
                let tolerance = self.commands.path_tolerance(path, coverage_to_root);
                let mesh = tessellate_stroke_with_tolerance(path, style, tolerance);
                Some(self.evaluate_geometry_mask(&mesh, coverage_to_root))
            }
        }
    }

    fn render_canvas_source(
        &mut self,
        accumulator: usize,
        content: &Canvas,
        coverage: &Coverage,
        draw_to_root: Projective2,
    ) {
        self.commands.end_rendering();
        let content_image = self.evaluate(content, draw_to_root);
        let mask_image = self.evaluate_coverage(coverage, draw_to_root);
        if let Some(mask_image) = mask_image {
            self.commands.apply_mask(content_image, mask_image);
        }
        self.commands.resume_accumulator(accumulator);
        self.commands
            .draw_composite(content_image, Composite::SourceOver);
    }

    fn evaluate_color_field(&mut self, source: &ColorField, source_to_root: Projective2) -> usize {
        if let ColorField::Canvas(canvas) = source {
            return self.evaluate(canvas, source_to_root);
        }

        let image = self.next_image;
        self.next_image += 1;
        self.commands
            .begin_accumulator(image, vk::AttachmentLoadOp::CLEAR);
        self.commands
            .render_unbounded_color_field(source, source_to_root);
        self.commands.end_rendering();
        self.commands.transition_to_sampled(image);
        image
    }

    fn evaluate_geometry_mask(&mut self, mesh: &Mesh, mask_to_root: Projective2) -> usize {
        let image = self.next_image;
        self.next_image += 1;
        self.commands
            .begin_accumulator(image, vk::AttachmentLoadOp::CLEAR);
        self.commands.render_mesh(
            mesh,
            &ColorField::Solid(Color::new(Srgb, [255_u8, 255, 255], Opacity::<u8>::ONE).into()),
            mask_to_root,
        );
        self.commands.end_rendering();
        self.commands.transition_to_sampled(image);
        image
    }
}
