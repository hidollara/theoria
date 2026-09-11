use ash::vk;

use crate::{
    canvas::{
        BlendMode, Canvas, CanvasNode, CanvasView, ColorField, Composite, Coverage, CoverageNode,
        Draw, ImagePattern,
    },
    color::Color,
    geometry::Point,
    path::Path,
    tessellation::{
        DEFAULT_TESSELLATION_TOLERANCE, Mesh, tessellate_fill_with_tolerance,
        tessellate_stroke_with_tolerance,
    },
    transform::Projective2,
};

use super::{
    canvas_plan::can_record_directly,
    encoding::{BlendCompositePushConstants, ImagePushConstants, TrianglePushConstants},
    full_viewport,
    mesh_paint::{PathSampler, paint_triangle_prepared, screen_space_tolerance},
    pipeline::{Pipeline, Pipelines},
};
use crate::vulkan::{
    image::ImageCache, image_layout::transition_image, scratch_image::ScratchImage,
};

pub(super) struct CanvasRecorder<'a> {
    pub(super) device: &'a ash::Device,
    pub(super) command_buffer: vk::CommandBuffer,
    pub(super) pipelines: &'a Pipelines,
    pub(super) image_cache: &'a ImageCache,
    pub(super) scratch_images: &'a [ScratchImage],
    pub(super) attachment_area: vk::Rect2D,
    pub(super) target_region: Option<(vk::Viewport, vk::Rect2D)>,
    pub(super) view: CanvasView,
    pub(super) next_image: usize,
}

impl CanvasRecorder<'_> {
    pub(super) fn evaluate(&mut self, canvas: &Canvas, canvas_to_root: Projective2) -> usize {
        let accumulator = self.next_image;
        self.next_image += 1;
        self.begin_accumulator(accumulator, vk::AttachmentLoadOp::CLEAR);
        self.render_node(accumulator, canvas, canvas_to_root);
        self.end_rendering();
        self.transition_to_sampled(accumulator);
        accumulator
    }

    fn render_node(&mut self, accumulator: usize, canvas: &Canvas, canvas_to_root: Projective2) {
        match canvas.node() {
            CanvasNode::Empty => {}
            CanvasNode::Draw(draw) => {
                self.render_draw(accumulator, draw, canvas_to_root);
            }
            CanvasNode::Transform { content, transform } => {
                self.render_node(accumulator, content, transform.then(canvas_to_root));
            }
            CanvasNode::Composite {
                backdrop,
                source,
                blend_mode,
                composite,
            } => {
                self.render_node(accumulator, backdrop, canvas_to_root);
                if *blend_mode == BlendMode::Normal
                    && *composite == Composite::SourceOver
                    && can_record_directly(source)
                {
                    self.render_node(accumulator, source, canvas_to_root);
                } else {
                    self.end_rendering();
                    let source_image = self.evaluate(source, canvas_to_root);
                    if *blend_mode == BlendMode::Normal {
                        self.resume_accumulator(accumulator);
                        self.draw_fullscreen(
                            self.pipelines.composite(*composite),
                            self.scratch_images[source_image].descriptor_set(),
                        );
                    } else {
                        self.blend_composite(accumulator, source_image, *blend_mode, *composite);
                    }
                }
            }
        }
    }

    fn render_draw(&mut self, accumulator: usize, draw: &Draw, draw_to_root: Projective2) {
        if draw.blend_mode != BlendMode::Normal || draw.composite != Composite::SourceOver {
            if !matches!(draw.coverage.node(), CoverageNode::All) {
                self.render_covered_composite(accumulator, draw, draw_to_root);
                return;
            }
            self.end_rendering();
            let source_image = self.next_image;
            self.next_image += 1;
            self.begin_accumulator(source_image, vk::AttachmentLoadOp::CLEAR);
            self.render_source_with_coverage(source_image, draw, draw_to_root);
            self.end_rendering();
            self.transition_to_sampled(source_image);
            if draw.blend_mode == BlendMode::Normal {
                self.resume_accumulator(accumulator);
                self.draw_fullscreen(
                    self.pipelines.composite(draw.composite),
                    self.scratch_images[source_image].descriptor_set(),
                );
            } else {
                self.blend_composite(accumulator, source_image, draw.blend_mode, draw.composite);
            }
            return;
        }
        self.render_source_with_coverage(accumulator, draw, draw_to_root);
    }

    fn render_covered_composite(
        &mut self,
        accumulator: usize,
        draw: &Draw,
        draw_to_root: Projective2,
    ) {
        self.end_rendering();
        let source = self.evaluate_color_field(&draw.source, draw_to_root);
        let coverage = self
            .evaluate_coverage(&draw.coverage, draw_to_root)
            .expect("non-All coverage evaluates to an image");

        self.transition_to_sampled(accumulator);
        let destination = self.next_image;
        self.next_image += 1;
        self.begin_accumulator(destination, vk::AttachmentLoadOp::CLEAR);
        self.draw_fullscreen(
            self.pipelines.composite(Composite::Copy),
            self.scratch_images[accumulator].descriptor_set(),
        );
        self.end_rendering();
        self.transition_to_sampled(destination);

        let candidate = self.next_image;
        self.next_image += 1;
        self.begin_accumulator(candidate, vk::AttachmentLoadOp::CLEAR);
        self.draw_blend_composite(destination, source, draw.blend_mode, draw.composite);
        self.end_rendering();
        self.transition_to_sampled(candidate);

        self.begin_accumulator(accumulator, vk::AttachmentLoadOp::CLEAR);
        unsafe {
            self.device.cmd_bind_pipeline(
                self.command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                self.pipelines.coverage.handle,
            );
            self.device.cmd_bind_descriptor_sets(
                self.command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                self.pipelines.coverage.layout,
                0,
                &[
                    self.scratch_images[destination].descriptor_set(),
                    self.scratch_images[candidate].descriptor_set(),
                    self.scratch_images[coverage].descriptor_set(),
                ],
                &[],
            );
            self.device.cmd_draw(self.command_buffer, 3, 1, 0, 0);
        }
    }

    fn render_source_with_coverage(
        &mut self,
        accumulator: usize,
        draw: &Draw,
        draw_to_root: Projective2,
    ) {
        if !self.set_canvas_region() {
            return;
        }

        if let ColorField::Canvas(content) = &draw.source {
            self.render_canvas_source(accumulator, content, &draw.coverage, draw_to_root);
            return;
        }

        match draw.coverage.node() {
            CoverageNode::All => self.render_unbounded_color_field(&draw.source, draw_to_root),
            CoverageNode::Fill { path, rule } => {
                let tolerance = self.path_tolerance(path, draw_to_root);
                let mesh = tessellate_fill_with_tolerance(path, *rule, tolerance);
                self.render_mesh(&mesh, &draw.source, draw_to_root);
            }
            CoverageNode::Stroke { path, style } => {
                let tolerance = self.path_tolerance(path, draw_to_root);
                let mesh = tessellate_stroke_with_tolerance(path, style, tolerance);
                self.render_mesh(&mesh, &draw.source, draw_to_root);
            }
            CoverageNode::AlphaOf(coverage_source) => {
                self.end_rendering();
                let content_image = self.evaluate_color_field(&draw.source, draw_to_root);
                let mask_image = self.evaluate_color_field(coverage_source, draw_to_root);
                self.apply_mask(content_image, mask_image);
                self.resume_accumulator(accumulator);
                self.draw_fullscreen(
                    self.pipelines.composite(Composite::SourceOver),
                    self.scratch_images[content_image].descriptor_set(),
                );
            }
        }
    }

    fn blend_composite(
        &mut self,
        accumulator: usize,
        source: usize,
        blend_mode: BlendMode,
        composite: Composite,
    ) {
        self.transition_to_sampled(accumulator);
        let destination = self.next_image;
        self.next_image += 1;
        self.begin_accumulator(destination, vk::AttachmentLoadOp::CLEAR);
        self.draw_fullscreen(
            self.pipelines.composite(Composite::Copy),
            self.scratch_images[accumulator].descriptor_set(),
        );
        self.end_rendering();
        self.transition_to_sampled(destination);

        self.begin_accumulator(accumulator, vk::AttachmentLoadOp::CLEAR);
        self.draw_blend_composite(destination, source, blend_mode, composite);
    }

    fn draw_blend_composite(
        &self,
        destination: usize,
        source: usize,
        blend_mode: BlendMode,
        composite: Composite,
    ) {
        let constants = BlendCompositePushConstants::new(blend_mode, composite);
        unsafe {
            self.device.cmd_bind_pipeline(
                self.command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                self.pipelines.blend.handle,
            );
            self.device.cmd_bind_descriptor_sets(
                self.command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                self.pipelines.blend.layout,
                0,
                &[
                    self.scratch_images[destination].descriptor_set(),
                    self.scratch_images[source].descriptor_set(),
                ],
                &[],
            );
            self.device.cmd_push_constants(
                self.command_buffer,
                self.pipelines.blend.layout,
                vk::ShaderStageFlags::FRAGMENT,
                0,
                constants.as_bytes(),
            );
            self.device.cmd_draw(self.command_buffer, 3, 1, 0, 0);
        }
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
                let tolerance = self.path_tolerance(path, coverage_to_root);
                let mesh = tessellate_fill_with_tolerance(path, *rule, tolerance);
                Some(self.evaluate_geometry_mask(&mesh, coverage_to_root))
            }
            CoverageNode::Stroke { path, style } => {
                let tolerance = self.path_tolerance(path, coverage_to_root);
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
        self.end_rendering();
        let content_image = self.evaluate(content, draw_to_root);
        let mask_image = match coverage.node() {
            CoverageNode::All => None,
            CoverageNode::AlphaOf(source) => Some(self.evaluate_color_field(source, draw_to_root)),
            CoverageNode::Fill { path, rule } => {
                let tolerance = self.path_tolerance(path, draw_to_root);
                let mesh = tessellate_fill_with_tolerance(path, *rule, tolerance);
                Some(self.evaluate_geometry_mask(&mesh, draw_to_root))
            }
            CoverageNode::Stroke { path, style } => {
                let tolerance = self.path_tolerance(path, draw_to_root);
                let mesh = tessellate_stroke_with_tolerance(path, style, tolerance);
                Some(self.evaluate_geometry_mask(&mesh, draw_to_root))
            }
        };
        if let Some(mask_image) = mask_image {
            self.apply_mask(content_image, mask_image);
        }
        self.resume_accumulator(accumulator);
        self.draw_fullscreen(
            self.pipelines.composite(Composite::SourceOver),
            self.scratch_images[content_image].descriptor_set(),
        );
    }

    fn evaluate_color_field(&mut self, source: &ColorField, source_to_root: Projective2) -> usize {
        if let ColorField::Canvas(canvas) = source {
            return self.evaluate(canvas, source_to_root);
        }

        let image = self.next_image;
        self.next_image += 1;
        self.begin_accumulator(image, vk::AttachmentLoadOp::CLEAR);
        self.render_unbounded_color_field(source, source_to_root);
        self.end_rendering();
        self.transition_to_sampled(image);
        image
    }

    fn render_unbounded_color_field(&self, source: &ColorField, source_to_root: Projective2) {
        let source_to_view = source_to_root.then(self.view.canvas_to_view);
        if let Some(view_to_source) = source_to_view.try_inverse() {
            let corners = [
                Point::new(0.0, 0.0),
                Point::new(1.0, 0.0),
                Point::new(1.0, 1.0),
                Point::new(0.0, 1.0),
            ];
            if let Some(vertices) = corners
                .map(|point| view_to_source.try_transform_point(point))
                .into_iter()
                .collect::<Option<Vec<_>>>()
            {
                self.render_mesh(
                    &Mesh {
                        vertices,
                        indices: vec![0, 1, 2, 2, 3, 0],
                    },
                    source,
                    source_to_root,
                );
            }
        }
    }

    fn evaluate_geometry_mask(&mut self, mesh: &Mesh, mask_to_root: Projective2) -> usize {
        let image = self.next_image;
        self.next_image += 1;
        self.begin_accumulator(image, vk::AttachmentLoadOp::CLEAR);
        self.render_mesh(
            mesh,
            &ColorField::Solid(Color::rgb8(255, 255, 255)),
            mask_to_root,
        );
        self.end_rendering();
        self.transition_to_sampled(image);
        image
    }

    fn render_mesh(&self, mesh: &Mesh, source: &ColorField, mesh_to_root: Projective2) {
        if let ColorField::Image(pattern) = source {
            self.render_image_mesh(mesh, pattern, mesh_to_root);
            return;
        }
        let path_sampler = match source {
            ColorField::AlongPathGradient(gradient) => Some(PathSampler::new(
                gradient.path(),
                self.path_tolerance(gradient.path(), mesh_to_root),
            )),
            ColorField::AcrossPathGradient(gradient) => Some(PathSampler::new(
                gradient.path(),
                self.path_tolerance(gradient.path(), mesh_to_root),
            )),
            _ => None,
        };
        unsafe {
            self.device.cmd_bind_pipeline(
                self.command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                self.pipelines.triangle.handle,
            );
            for triangle in mesh.indices.chunks_exact(3) {
                let points = [
                    mesh.vertices[triangle[0] as usize],
                    mesh.vertices[triangle[1] as usize],
                    mesh.vertices[triangle[2] as usize],
                ];
                for painted in paint_triangle_prepared(points, source, path_sampler.as_ref()) {
                    let constants = TrianglePushConstants::new(
                        painted.points,
                        painted.colors,
                        mesh_to_root,
                        self.view,
                    );
                    self.device.cmd_push_constants(
                        self.command_buffer,
                        self.pipelines.triangle.layout,
                        vk::ShaderStageFlags::VERTEX,
                        0,
                        constants.as_bytes(),
                    );
                    self.device.cmd_draw(self.command_buffer, 3, 1, 0, 0);
                }
            }
        }
    }

    fn render_image_mesh(&self, mesh: &Mesh, pattern: &ImagePattern, mesh_to_root: Projective2) {
        let gpu_image = self.image_cache.get(pattern.image()).unwrap();
        unsafe {
            self.device.cmd_bind_pipeline(
                self.command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                self.pipelines.image.handle,
            );
            self.device.cmd_bind_descriptor_sets(
                self.command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                self.pipelines.image.layout,
                0,
                &[gpu_image.descriptor_set(pattern)],
                &[],
            );
            for triangle in mesh.indices.chunks_exact(3) {
                let points = [
                    mesh.vertices[triangle[0] as usize],
                    mesh.vertices[triangle[1] as usize],
                    mesh.vertices[triangle[2] as usize],
                ];
                let constants = ImagePushConstants::new(
                    pattern,
                    gpu_image.alpha_mode(),
                    points,
                    mesh_to_root,
                    self.view,
                );
                self.device.cmd_push_constants(
                    self.command_buffer,
                    self.pipelines.image.layout,
                    vk::ShaderStageFlags::VERTEX,
                    0,
                    constants.as_bytes(),
                );
                self.device.cmd_draw(self.command_buffer, 3, 1, 0, 0);
            }
        }
    }

    fn apply_mask(&self, content: usize, mask: usize) {
        let content_image = &self.scratch_images[content];
        let sampled_destination_access = if content_image.is_multisampled() {
            vk::AccessFlags2::COLOR_ATTACHMENT_WRITE
        } else {
            vk::AccessFlags2::COLOR_ATTACHMENT_READ | vk::AccessFlags2::COLOR_ATTACHMENT_WRITE
        };
        unsafe {
            transition_image(
                self.device,
                self.command_buffer,
                content_image.sampled_image(),
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                vk::PipelineStageFlags2::FRAGMENT_SHADER,
                vk::AccessFlags2::SHADER_SAMPLED_READ,
                vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                sampled_destination_access,
            );
            if content_image.is_multisampled() {
                transition_image(
                    self.device,
                    self.command_buffer,
                    content_image.attachment_image(),
                    vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                    vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                    vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                    vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
                    vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                    vk::AccessFlags2::COLOR_ATTACHMENT_READ
                        | vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
                );
            }
        }
        self.begin_accumulator(content, vk::AttachmentLoadOp::LOAD);
        self.draw_fullscreen(
            &self.pipelines.mask,
            self.scratch_images[mask].descriptor_set(),
        );
        self.end_rendering();
        self.transition_to_sampled(content);
    }

    fn transition_to_sampled(&self, index: usize) {
        unsafe {
            transition_image(
                self.device,
                self.command_buffer,
                self.scratch_images[index].sampled_image(),
                vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
                vk::PipelineStageFlags2::FRAGMENT_SHADER,
                vk::AccessFlags2::SHADER_SAMPLED_READ,
            );
        }
    }

    fn begin_accumulator(&self, index: usize, load_op: vk::AttachmentLoadOp) {
        let image = &self.scratch_images[index];
        if load_op == vk::AttachmentLoadOp::CLEAR {
            unsafe {
                transition_image(
                    self.device,
                    self.command_buffer,
                    image.attachment_image(),
                    vk::ImageLayout::UNDEFINED,
                    vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                    vk::PipelineStageFlags2::NONE,
                    vk::AccessFlags2::NONE,
                    vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                    vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
                );
                if image.is_multisampled() {
                    transition_image(
                        self.device,
                        self.command_buffer,
                        image.sampled_image(),
                        vk::ImageLayout::UNDEFINED,
                        vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                        vk::PipelineStageFlags2::NONE,
                        vk::AccessFlags2::NONE,
                        vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                        vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
                    );
                }
            }
        }
        let mut attachment = vk::RenderingAttachmentInfo::default()
            .image_view(image.attachment_view())
            .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .load_op(load_op)
            .store_op(vk::AttachmentStoreOp::STORE)
            .clear_value(vk::ClearValue {
                color: vk::ClearColorValue { float32: [0.0; 4] },
            });
        if image.is_multisampled() {
            attachment = attachment
                .resolve_mode(vk::ResolveModeFlags::AVERAGE)
                .resolve_image_view(image.sampled_view())
                .resolve_image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL);
        }
        let rendering = vk::RenderingInfo::default()
            .render_area(self.attachment_area)
            .layer_count(1)
            .color_attachments(std::slice::from_ref(&attachment));
        unsafe {
            self.device
                .cmd_begin_rendering(self.command_buffer, &rendering);
        }
    }

    fn resume_accumulator(&self, index: usize) {
        unsafe {
            transition_image(
                self.device,
                self.command_buffer,
                self.scratch_images[index].attachment_image(),
                vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
                vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                vk::AccessFlags2::COLOR_ATTACHMENT_READ | vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
            );
        }
        self.begin_accumulator(index, vk::AttachmentLoadOp::LOAD);
    }

    fn path_tolerance(&self, path: &Path, path_to_root: Projective2) -> f32 {
        let Some((viewport, _)) = self.target_region else {
            return DEFAULT_TESSELLATION_TOLERANCE;
        };
        let path_to_view = path_to_root.then(self.view.canvas_to_view);
        screen_space_tolerance(path, path_to_view, viewport.width, viewport.height)
    }

    fn set_canvas_region(&self) -> bool {
        let Some((viewport, scissor)) = self.target_region else {
            return false;
        };
        unsafe {
            self.device
                .cmd_set_viewport(self.command_buffer, 0, &[viewport]);
            self.device
                .cmd_set_scissor(self.command_buffer, 0, &[scissor]);
        }
        true
    }

    fn draw_fullscreen(&self, pipeline: &Pipeline, descriptor_set: vk::DescriptorSet) {
        let extent = self.attachment_area.extent;
        unsafe {
            self.device
                .cmd_set_viewport(self.command_buffer, 0, &[full_viewport(extent)]);
            self.device
                .cmd_set_scissor(self.command_buffer, 0, &[self.attachment_area]);
            self.device.cmd_bind_pipeline(
                self.command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                pipeline.handle,
            );
            self.device.cmd_bind_descriptor_sets(
                self.command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                pipeline.layout,
                0,
                &[descriptor_set],
                &[],
            );
            self.device.cmd_draw(self.command_buffer, 3, 1, 0, 0);
        }
    }

    fn end_rendering(&self) {
        unsafe {
            self.device.cmd_end_rendering(self.command_buffer);
        }
    }
}
