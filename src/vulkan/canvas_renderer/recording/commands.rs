//! Vulkan commands used while evaluating a Canvas recording plan.

use ash::vk;

use crate::{
    canvas::{CanvasView, ColorField, Composite, Composition, ImagePattern},
    canvas_renderer::{
        color_space::PreparedColors, mesh_paint::paint_triangle_prepared,
        path_sampling::PathSampler, view_space::screen_space_tolerance,
    },
    geometry::Point,
    path::Path,
    tessellation::{DEFAULT_TESSELLATION_TOLERANCE, Mesh},
    transform::Projective2,
    vulkan::{
        canvas_renderer::{
            encoding::{CompositionPushConstants, ImagePushConstants, TrianglePushConstants},
            pipeline::{Pipeline, Pipelines},
        },
        color::PremultipliedWorkingRgba,
        image::{GpuImageColor, ImageCache},
        image_layout::transition_image,
        scratch_image::ScratchImage,
    },
};

pub(super) struct CanvasCommandEncoder<'a> {
    device: &'a ash::Device,
    command_buffer: vk::CommandBuffer,
    pipelines: &'a Pipelines,
    image_cache: &'a ImageCache,
    scratch_images: &'a [ScratchImage],
    attachment_area: vk::Rect2D,
    target_region: Option<(vk::Viewport, vk::Rect2D)>,
    view: CanvasView,
    prepared: &'a PreparedColors,
}

impl<'a> CanvasCommandEncoder<'a> {
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
            device,
            command_buffer,
            pipelines,
            image_cache,
            scratch_images,
            attachment_area,
            target_region,
            view,
            prepared,
        }
    }

    pub(super) fn draw_composite(&self, source: usize, composite: Composite) {
        self.draw_fullscreen(
            self.pipelines.composite(composite),
            self.scratch_images[source].descriptor_set(),
        );
    }

    pub(super) fn present(
        &self,
        source: usize,
        image_view: vk::ImageView,
        clear_color: PremultipliedWorkingRgba,
        pipeline: &Pipeline,
    ) {
        let color_attachment = vk::RenderingAttachmentInfo::default()
            .image_view(image_view)
            .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .clear_value(vk::ClearValue {
                color: vk::ClearColorValue {
                    float32: clear_color.components(),
                },
            });
        let rendering_info = vk::RenderingInfo::default()
            .render_area(self.attachment_area)
            .layer_count(1)
            .color_attachments(std::slice::from_ref(&color_attachment));
        unsafe {
            self.device
                .cmd_begin_rendering(self.command_buffer, &rendering_info);
        }
        self.draw_fullscreen(pipeline, self.scratch_images[source].descriptor_set());
        self.end_rendering();
    }

    pub(super) fn draw_composition(
        &self,
        destination: usize,
        source: usize,
        composition: &Composition,
    ) {
        let constants = CompositionPushConstants::new(composition);
        let pipeline = self.pipelines.blend(composition.blend_shader());
        unsafe {
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
                &[
                    self.scratch_images[destination].descriptor_set(),
                    self.scratch_images[source].descriptor_set(),
                ],
                &[],
            );
            self.device.cmd_push_constants(
                self.command_buffer,
                pipeline.layout,
                vk::ShaderStageFlags::FRAGMENT,
                0,
                constants.as_bytes(),
            );
            self.device.cmd_draw(self.command_buffer, 3, 1, 0, 0);
        }
    }

    pub(super) fn render_unbounded_color_field(
        &self,
        source: &ColorField,
        source_to_root: Projective2,
    ) {
        if matches!(source, ColorField::Transparent) {
            return;
        }
        if let Some(mesh) = self.unbounded_mesh(source_to_root) {
            self.render_mesh(&mesh, source, source_to_root);
        }
    }

    fn unbounded_mesh(&self, source_to_root: Projective2) -> Option<Mesh> {
        let source_to_view = source_to_root.then(self.view.canvas_to_view);
        let view_to_source = source_to_view.try_inverse()?;
        let corners = [
            Point::new(0.0, 0.0),
            Point::new(1.0, 0.0),
            Point::new(1.0, 1.0),
            Point::new(0.0, 1.0),
        ];
        let vertices = corners
            .map(|point| view_to_source.try_transform_point(point))
            .into_iter()
            .collect::<Option<Vec<_>>>()?;
        Some(Mesh {
            vertices,
            indices: vec![0, 1, 2, 2, 3, 0],
        })
    }

    pub(super) fn render_mesh(&self, mesh: &Mesh, source: &ColorField, mesh_to_root: Projective2) {
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
                for painted in
                    paint_triangle_prepared(points, source, path_sampler.as_ref(), self.prepared)
                {
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
        let (pipeline, icc) = match &gpu_image.color {
            GpuImageColor::Rgb { .. } => (&self.pipelines.image, false),
            GpuImageColor::Icc { key, .. } => (self.pipelines.icc_image(key), true),
        };
        unsafe {
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
                &[gpu_image.descriptor_set(pattern)],
                &[],
            );
            for triangle in mesh.indices.chunks_exact(3) {
                let points = [
                    mesh.vertices[triangle[0] as usize],
                    mesh.vertices[triangle[1] as usize],
                    mesh.vertices[triangle[2] as usize],
                ];
                let constants = if icc {
                    ImagePushConstants::new_icc(pattern, points, mesh_to_root, self.view)
                } else {
                    let GpuImageColor::Rgb { association, space } = &gpu_image.color else {
                        unreachable!()
                    };
                    ImagePushConstants::new(
                        pattern,
                        *association,
                        *space,
                        points,
                        mesh_to_root,
                        self.view,
                    )
                };
                self.device.cmd_push_constants(
                    self.command_buffer,
                    pipeline.layout,
                    vk::ShaderStageFlags::VERTEX,
                    0,
                    constants.as_bytes(),
                );
                self.device.cmd_draw(self.command_buffer, 3, 1, 0, 0);
            }
        }
    }

    pub(super) fn apply_mask(&self, content: usize, mask: usize) {
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

    pub(super) fn transition_to_sampled(&self, index: usize) {
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

    pub(super) fn begin_accumulator(&self, index: usize, load_op: vk::AttachmentLoadOp) {
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

    pub(super) fn resume_accumulator(&self, index: usize) {
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

    pub(super) fn path_tolerance(&self, path: &Path, path_to_root: Projective2) -> f32 {
        let Some((viewport, _)) = self.target_region else {
            return DEFAULT_TESSELLATION_TOLERANCE;
        };
        let path_to_view = path_to_root.then(self.view.canvas_to_view);
        screen_space_tolerance(path, path_to_view, viewport.width, viewport.height)
    }

    pub(super) fn set_canvas_region(&self) -> bool {
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

    pub(super) fn end_rendering(&self) {
        unsafe {
            self.device.cmd_end_rendering(self.command_buffer);
        }
    }
}

fn full_viewport(extent: vk::Extent2D) -> vk::Viewport {
    vk::Viewport {
        x: 0.0,
        y: 0.0,
        width: extent.width as f32,
        height: extent.height as f32,
        min_depth: 0.0,
        max_depth: 1.0,
    }
}
