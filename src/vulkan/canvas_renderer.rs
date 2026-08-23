use std::{collections::HashMap, error::Error, ffi::CStr, io::Cursor, mem::size_of, slice};

use ash::vk;

use crate::{
    canvas::{
        Canvas, CanvasNode, CanvasView, Color, CompositeOperator, Coverage, CoverageNode, Draw,
        GradientPoint, GradientStop, ImagePattern, LinearGradient, MeshGradient, Source,
    },
    geometry::Point,
    path::{Path, PathSegment},
    target::Viewport,
    tessellation::{
        DEFAULT_TESSELLATION_TOLERANCE, Mesh, flatten_subpaths, tessellate_fill_with_tolerance,
        tessellate_stroke_with_tolerance,
    },
    transform::Projective2,
};

use super::{
    image::ImageCache,
    presenter::{RenderDevice, RenderTarget},
    render_image::RenderImage,
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

const TRIANGLE_VERTEX_SHADER: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/triangle.vert.spv"));
const TRIANGLE_FRAGMENT_SHADER: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/triangle.frag.spv"));
const IMAGE_VERTEX_SHADER: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/image.vert.spv"));
const IMAGE_FRAGMENT_SHADER: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/image.frag.spv"));
const COMPOSITE_VERTEX_SHADER: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/composite.vert.spv"));
const COMPOSITE_FRAGMENT_SHADER: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/composite.frag.spv"));
const MASK_VERTEX_SHADER: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/mask.vert.spv"));
const MASK_FRAGMENT_SHADER: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/mask.frag.spv"));

#[repr(C)]
#[derive(Clone, Copy)]
struct GpuVertex {
    position: [f32; 4],
    color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct TrianglePushConstants {
    vertices: [GpuVertex; 3],
}

impl TrianglePushConstants {
    fn new(
        points: [Point; 3],
        colors: [[f32; 4]; 3],
        canvas_to_root: Projective2,
        view: CanvasView,
    ) -> Self {
        let canvas_to_view = canvas_to_root.then(view.canvas_to_view);
        Self {
            vertices: std::array::from_fn(|index| GpuVertex {
                position: transformed_point_to_clip(points[index], canvas_to_view),
                color: colors[index],
            }),
        }
    }

    fn as_bytes(&self) -> &[u8] {
        unsafe { slice::from_raw_parts((self as *const Self).cast(), size_of::<Self>()) }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
struct ImagePushConstants {
    vertices: [ImageVertex; 3],
}

impl ImagePushConstants {
    fn new(
        pattern: &ImagePattern,
        points: [Point; 3],
        image_to_root: Projective2,
        view: CanvasView,
    ) -> Self {
        let width = pattern.image().width() as f32;
        let height = pattern.image().height() as f32;
        let image_to_view = image_to_root.then(view.canvas_to_view);
        Self {
            vertices: points.map(|point| {
                let image_point = pattern.canvas_to_image().transform_homogeneous(point);
                ImageVertex {
                    position: transformed_point_to_clip(point, image_to_view),
                    uv: [
                        image_point.x / width,
                        image_point.y / height,
                        image_point.z,
                        0.0,
                    ],
                }
            }),
        }
    }

    fn as_bytes(&self) -> &[u8] {
        unsafe { slice::from_raw_parts((self as *const Self).cast(), size_of::<Self>()) }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
struct ImageVertex {
    position: [f32; 4],
    uv: [f32; 4],
}

struct Pipeline {
    layout: vk::PipelineLayout,
    handle: vk::Pipeline,
}

struct Pipelines {
    format: vk::Format,
    sample_count: vk::SampleCountFlags,
    triangle: Pipeline,
    image: Pipeline,
    composites: HashMap<CompositeOperator, Pipeline>,
    mask: Pipeline,
    present: Pipeline,
}

impl Pipelines {
    fn composite(&self, operator: CompositeOperator) -> &Pipeline {
        &self.composites[&operator]
    }
}

/// Translates backend-independent Canvas commands into Vulkan commands.
pub struct CanvasRenderer {
    device: RenderDevice,
    pipelines: Option<Pipelines>,
    image_cache: Option<ImageCache>,
    render_images: Vec<RenderImage>,
}

impl CanvasRenderer {
    pub(crate) fn new(device: RenderDevice) -> Self {
        Self {
            device,
            pipelines: None,
            image_cache: None,
            render_images: Vec::new(),
        }
    }

    pub(crate) fn render(
        &mut self,
        target: RenderTarget<'_>,
        viewport: Viewport,
        canvas: &Canvas,
        view: CanvasView,
    ) -> Result<()> {
        let (image_view, format, extent) = target.surface();
        let sample_count = preferred_sample_count(&self.device, format);
        let target_region = target_region(viewport, extent);
        self.ensure_image_cache()?;
        upload_canvas_images(self.image_cache.as_mut().unwrap(), canvas)?;
        self.ensure_pipelines(format, sample_count)?;
        self.ensure_render_images(format, extent, sample_count, required_render_images(canvas))?;
        let pipelines = self.pipelines.as_ref().unwrap();
        let image_cache = self.image_cache.as_ref().unwrap();
        let device = target.device.raw();
        let command_buffer = target.command_buffer;
        let clear_color = target.clear_color();
        let attachment_area = vk::Rect2D {
            offset: vk::Offset2D::default(),
            extent,
        };

        unsafe {
            let mut recorder = CanvasRecorder {
                device,
                command_buffer,
                pipelines,
                image_cache,
                render_images: &self.render_images,
                attachment_area,
                target_region,
                view,
                next_image: 0,
            };
            let canvas_image = recorder.evaluate(canvas, Projective2::IDENTITY);

            let color_attachment = vk::RenderingAttachmentInfo::default()
                .image_view(image_view)
                .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::STORE)
                .clear_value(vk::ClearValue {
                    color: vk::ClearColorValue {
                        float32: clear_color.to_premultiplied_array(),
                    },
                });
            let rendering_info = vk::RenderingInfo::default()
                .render_area(attachment_area)
                .layer_count(1)
                .color_attachments(std::slice::from_ref(&color_attachment));
            device.cmd_begin_rendering(command_buffer, &rendering_info);
            device.cmd_set_viewport(command_buffer, 0, &[full_viewport(extent)]);
            device.cmd_set_scissor(command_buffer, 0, &[attachment_area]);
            device.cmd_bind_pipeline(
                command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                pipelines.present.handle,
            );
            device.cmd_bind_descriptor_sets(
                command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                pipelines.present.layout,
                0,
                &[self.render_images[canvas_image].descriptor_set()],
                &[],
            );
            device.cmd_draw(command_buffer, 3, 1, 0, 0);
            device.cmd_end_rendering(command_buffer);
        }
        Ok(())
    }

    fn ensure_image_cache(&mut self) -> Result<()> {
        if self.image_cache.is_none() {
            self.image_cache = Some(ImageCache::new(self.device.clone())?);
        }
        Ok(())
    }

    fn ensure_pipelines(
        &mut self,
        format: vk::Format,
        sample_count: vk::SampleCountFlags,
    ) -> Result<()> {
        if self.pipelines.as_ref().is_some_and(|pipelines| {
            pipelines.format == format && pipelines.sample_count == sample_count
        }) {
            return Ok(());
        }
        self.destroy_pipelines();
        let descriptor_layout = self.image_cache.as_ref().unwrap().descriptor_set_layout();
        self.pipelines = Some(unsafe {
            create_pipelines(self.device.raw(), format, sample_count, descriptor_layout)?
        });
        Ok(())
    }

    fn ensure_render_images(
        &mut self,
        format: vk::Format,
        extent: vk::Extent2D,
        sample_count: vk::SampleCountFlags,
        required: usize,
    ) -> Result<()> {
        if self.render_images.first().is_some_and(|image| {
            image.format != format || image.extent != extent || image.sample_count != sample_count
        }) {
            self.render_images.clear();
        }
        while self.render_images.len() < required {
            self.render_images.push(RenderImage::new(
                self.device.clone(),
                self.image_cache.as_ref().unwrap(),
                format,
                extent,
                sample_count,
            )?);
        }
        Ok(())
    }

    fn destroy_pipelines(&mut self) {
        if let Some(pipelines) = self.pipelines.take() {
            unsafe {
                for pipeline in [
                    pipelines.triangle,
                    pipelines.image,
                    pipelines.mask,
                    pipelines.present,
                ]
                .into_iter()
                .chain(pipelines.composites.into_values())
                {
                    self.device.raw().destroy_pipeline(pipeline.handle, None);
                    self.device
                        .raw()
                        .destroy_pipeline_layout(pipeline.layout, None);
                }
            }
        }
    }
}

fn upload_canvas_images(image_cache: &mut ImageCache, canvas: &Canvas) -> Result<()> {
    match canvas.node() {
        CanvasNode::Empty => {}
        CanvasNode::Draw(draw) => upload_draw_images(image_cache, draw)?,
        CanvasNode::Transform { content, .. } => upload_canvas_images(image_cache, content)?,
        CanvasNode::Composite {
            backdrop, source, ..
        } => {
            upload_canvas_images(image_cache, backdrop)?;
            upload_canvas_images(image_cache, source)?;
        }
    }
    Ok(())
}

fn upload_draw_images(image_cache: &mut ImageCache, draw: &Draw) -> Result<()> {
    upload_source_images(image_cache, &draw.source)?;
    if let CoverageNode::SourceAlpha(source) = draw.coverage.node() {
        upload_source_images(image_cache, source)?;
    }
    Ok(())
}

fn upload_source_images(image_cache: &mut ImageCache, source: &Source) -> Result<()> {
    match source {
        Source::Image(pattern) => {
            image_cache.get_or_upload(pattern.image())?;
        }
        Source::Canvas(canvas) => upload_canvas_images(image_cache, canvas)?,
        Source::Solid(_)
        | Source::LinearGradient(_)
        | Source::RadialGradient(_)
        | Source::ConicGradient(_)
        | Source::AlongPathGradient(_)
        | Source::AcrossPathGradient(_)
        | Source::MeshGradient(_) => {}
    }
    Ok(())
}

fn required_render_images(canvas: &Canvas) -> usize {
    1 + additional_render_images(canvas)
}

fn additional_render_images(canvas: &Canvas) -> usize {
    match canvas.node() {
        CanvasNode::Empty => 0,
        CanvasNode::Draw(draw) => match &draw.source {
            Source::Canvas(content) => {
                required_render_images(content)
                    + match draw.coverage.node() {
                        CoverageNode::All => 0,
                        CoverageNode::Fill { .. } | CoverageNode::Stroke { .. } => 1,
                        CoverageNode::SourceAlpha(source) => required_source_images(source),
                    }
            }
            _ => match draw.coverage.node() {
                CoverageNode::SourceAlpha(source) => 1 + required_source_images(source),
                CoverageNode::All | CoverageNode::Fill { .. } | CoverageNode::Stroke { .. } => 0,
            },
        },
        CanvasNode::Transform { content, .. } => additional_render_images(content),
        CanvasNode::Composite {
            backdrop,
            source,
            operator,
        } => {
            additional_render_images(backdrop)
                + if *operator == CompositeOperator::SourceOver
                    && can_render_source_over_directly(source)
                {
                    additional_render_images(source)
                } else {
                    required_render_images(source)
                }
        }
    }
}

fn can_render_source_over_directly(canvas: &Canvas) -> bool {
    match canvas.node() {
        CanvasNode::Empty | CanvasNode::Draw(_) => true,
        CanvasNode::Transform { content, .. } => can_render_source_over_directly(content),
        CanvasNode::Composite {
            backdrop,
            source,
            operator: CompositeOperator::SourceOver,
        } => can_render_source_over_directly(backdrop) && can_render_source_over_directly(source),
        CanvasNode::Composite { .. } => false,
    }
}

fn required_source_images(source: &Source) -> usize {
    match source {
        Source::Canvas(canvas) => required_render_images(canvas),
        _ => 1,
    }
}

struct CanvasRecorder<'a> {
    device: &'a ash::Device,
    command_buffer: vk::CommandBuffer,
    pipelines: &'a Pipelines,
    image_cache: &'a ImageCache,
    render_images: &'a [RenderImage],
    attachment_area: vk::Rect2D,
    target_region: Option<(vk::Viewport, vk::Rect2D)>,
    view: CanvasView,
    next_image: usize,
}

impl CanvasRecorder<'_> {
    fn evaluate(&mut self, canvas: &Canvas, canvas_to_root: Projective2) -> usize {
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
                operator,
            } => {
                self.render_node(accumulator, backdrop, canvas_to_root);
                if *operator == CompositeOperator::SourceOver
                    && can_render_source_over_directly(source)
                {
                    self.render_node(accumulator, source, canvas_to_root);
                } else {
                    self.end_rendering();
                    let source_image = self.evaluate(source, canvas_to_root);
                    self.resume_accumulator(accumulator);
                    self.draw_fullscreen(
                        self.pipelines.composite(*operator),
                        self.render_images[source_image].descriptor_set(),
                    );
                }
            }
        }
    }

    fn render_draw(&mut self, accumulator: usize, draw: &Draw, draw_to_root: Projective2) {
        if !self.set_canvas_region() {
            return;
        }

        if let Source::Canvas(content) = &draw.source {
            self.render_canvas_source(accumulator, content, &draw.coverage, draw_to_root);
            return;
        }

        match draw.coverage.node() {
            CoverageNode::All => self.render_unbounded_source(&draw.source, draw_to_root),
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
            CoverageNode::SourceAlpha(coverage_source) => {
                self.end_rendering();
                let content_image = self.evaluate_source(&draw.source, draw_to_root);
                let mask_image = self.evaluate_source(coverage_source, draw_to_root);
                self.apply_mask(content_image, mask_image);
                self.resume_accumulator(accumulator);
                self.draw_fullscreen(
                    self.pipelines.composite(CompositeOperator::SourceOver),
                    self.render_images[content_image].descriptor_set(),
                );
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
            CoverageNode::SourceAlpha(source) => Some(self.evaluate_source(source, draw_to_root)),
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
            self.pipelines.composite(CompositeOperator::SourceOver),
            self.render_images[content_image].descriptor_set(),
        );
    }

    fn evaluate_source(&mut self, source: &Source, source_to_root: Projective2) -> usize {
        if let Source::Canvas(canvas) = source {
            return self.evaluate(canvas, source_to_root);
        }

        let image = self.next_image;
        self.next_image += 1;
        self.begin_accumulator(image, vk::AttachmentLoadOp::CLEAR);
        self.render_unbounded_source(source, source_to_root);
        self.end_rendering();
        self.transition_to_sampled(image);
        image
    }

    fn render_unbounded_source(&self, source: &Source, source_to_root: Projective2) {
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
            &Source::Solid(Color::rgb8(255, 255, 255)),
            mask_to_root,
        );
        self.end_rendering();
        self.transition_to_sampled(image);
        image
    }

    fn render_mesh(&self, mesh: &Mesh, source: &Source, mesh_to_root: Projective2) {
        if let Source::Image(pattern) = source {
            self.render_image_mesh(mesh, pattern, mesh_to_root);
            return;
        }
        let path_sampler = match source {
            Source::AlongPathGradient(gradient) => Some(PathSampler::new(
                gradient.path(),
                self.path_tolerance(gradient.path(), mesh_to_root),
            )),
            Source::AcrossPathGradient(gradient) => Some(PathSampler::new(
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
                let constants = ImagePushConstants::new(pattern, points, mesh_to_root, self.view);
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
        let content_image = &self.render_images[content];
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
            self.render_images[mask].descriptor_set(),
        );
        self.end_rendering();
        self.transition_to_sampled(content);
    }

    fn transition_to_sampled(&self, index: usize) {
        unsafe {
            transition_image(
                self.device,
                self.command_buffer,
                self.render_images[index].sampled_image(),
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
        let image = &self.render_images[index];
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
                self.render_images[index].attachment_image(),
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

impl Drop for CanvasRenderer {
    fn drop(&mut self) {
        unsafe {
            let _ = self.device.raw().device_wait_idle();
        }
        self.render_images.clear();
        self.destroy_pipelines();
        self.image_cache = None;
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct PaintedTriangle {
    points: [Point; 3],
    colors: [[f32; 4]; 3],
}

#[derive(Clone, Copy)]
struct GradientVertex {
    point: Point,
    offset: f32,
}

#[cfg(test)]
fn paint_triangle(points: [Point; 3], source: &Source) -> Vec<PaintedTriangle> {
    let path_sampler = match source {
        Source::AlongPathGradient(gradient) => Some(PathSampler::new(
            gradient.path(),
            DEFAULT_TESSELLATION_TOLERANCE,
        )),
        Source::AcrossPathGradient(gradient) => Some(PathSampler::new(
            gradient.path(),
            DEFAULT_TESSELLATION_TOLERANCE,
        )),
        _ => None,
    };
    paint_triangle_prepared(points, source, path_sampler.as_ref())
}

fn paint_triangle_prepared(
    points: [Point; 3],
    source: &Source,
    path_sampler: Option<&PathSampler>,
) -> Vec<PaintedTriangle> {
    match source {
        Source::Solid(color) => vec![PaintedTriangle {
            points,
            colors: [color.to_premultiplied_array(); 3],
        }],
        Source::LinearGradient(gradient) => paint_linear_gradient(points, gradient),
        Source::RadialGradient(gradient) => paint_sampled_gradient(points, |point| {
            let offset = (point - gradient.center()).length() / gradient.radius();
            evaluate_gradient_stops(gradient.stops(), offset)
        }),
        Source::ConicGradient(gradient) => paint_sampled_gradient(points, |point| {
            let delta = point - gradient.center();
            let angle = delta.y.atan2(delta.x);
            let offset = (angle - gradient.start_angle()).rem_euclid(std::f32::consts::TAU)
                / std::f32::consts::TAU;
            evaluate_gradient_stops(gradient.stops(), offset)
        }),
        Source::AlongPathGradient(gradient) => {
            let sampler = path_sampler.expect("along-Path sources prepare their Path once");
            paint_sampled_gradient(points, |point| {
                let offset = sampler.nearest_offset(point).unwrap_or(0.0);
                evaluate_gradient_stops(gradient.ramp().stops(), offset)
            })
        }
        Source::AcrossPathGradient(gradient) => {
            let sampler = path_sampler.expect("across-Path sources prepare their Path once");
            paint_sampled_gradient(points, |point| {
                let distance = sampler
                    .nearest_sample(point)
                    .map_or(gradient.from_distance(), |sample| sample.signed_distance);
                let offset = (distance - gradient.from_distance())
                    / (gradient.to_distance() - gradient.from_distance());
                evaluate_gradient_stops(gradient.ramp().stops(), offset)
            })
        }
        Source::MeshGradient(gradient) => paint_mesh_gradient(points, gradient),
        Source::Image(_) => unreachable!("image sources use the sampled-image pipeline"),
        Source::Canvas(_) => unreachable!("Canvas sources are evaluated before mesh drawing"),
    }
}

struct PathSampler {
    subpaths: Vec<MeasuredSubpath>,
}

struct MeasuredSubpath {
    segments: Vec<MeasuredSegment>,
    length: f32,
}

struct MeasuredSegment {
    from: Point,
    to: Point,
    start: f32,
    length: f32,
}

#[derive(Clone, Copy, Debug)]
struct PathSample {
    offset: f32,
    signed_distance: f32,
}

impl PathSampler {
    fn new(path: &Path, tolerance: f32) -> Self {
        let subpaths = flatten_subpaths(path, tolerance)
            .into_iter()
            .map(|points| {
                let mut length = 0.0;
                let segments = points
                    .windows(2)
                    .filter_map(|pair| {
                        let from = pair[0];
                        let to = pair[1];
                        let segment_length = (to - from).length();
                        if segment_length == 0.0 || !segment_length.is_finite() {
                            return None;
                        }
                        let segment = MeasuredSegment {
                            from,
                            to,
                            start: length,
                            length: segment_length,
                        };
                        length += segment_length;
                        Some(segment)
                    })
                    .collect();
                MeasuredSubpath { segments, length }
            })
            .filter(|subpath| subpath.length > 0.0)
            .collect();
        Self { subpaths }
    }

    fn nearest_offset(&self, point: Point) -> Option<f32> {
        self.nearest_sample(point).map(|sample| sample.offset)
    }

    fn nearest_sample(&self, point: Point) -> Option<PathSample> {
        let mut nearest: Option<(f32, PathSample)> = None;
        for subpath in &self.subpaths {
            for segment in &subpath.segments {
                let direction = segment.to - segment.from;
                let amount = ((point - segment.from).dot(direction)
                    / (segment.length * segment.length))
                    .clamp(0.0, 1.0);
                let closest = segment.from + direction * amount;
                let displacement = point - closest;
                let distance_squared = displacement.dot(displacement);
                let offset = (segment.start + segment.length * amount) / subpath.length;
                let signed_distance =
                    (direction.x * displacement.y - direction.y * displacement.x) / segment.length;
                let sample = PathSample {
                    offset,
                    signed_distance,
                };
                if nearest.is_none_or(|(best_distance, _)| distance_squared < best_distance) {
                    nearest = Some((distance_squared, sample));
                }
            }
        }
        nearest.map(|(_, sample)| sample)
    }
}

fn paint_linear_gradient(points: [Point; 3], gradient: &LinearGradient) -> Vec<PaintedTriangle> {
    let axis = gradient.end() - gradient.start();
    let length_squared = axis.dot(axis);
    let original: Vec<_> = points
        .map(|point| GradientVertex {
            point,
            offset: (point - gradient.start()).dot(axis) / length_squared,
        })
        .into();
    let stops = gradient.stops();
    let mut output = Vec::new();
    let mut remaining = original;

    let first = stops[0];
    let before = clip_gradient_polygon(&remaining, first.offset(), true);
    append_gradient_polygon(
        &mut output,
        &before,
        first.offset(),
        first.offset(),
        first.color().to_premultiplied_array(),
        first.color().to_premultiplied_array(),
    );
    remaining = clip_gradient_polygon(&remaining, first.offset(), false);

    for pair in stops.windows(2) {
        let [from, to] = [pair[0], pair[1]];
        let band = clip_gradient_polygon(&remaining, to.offset(), true);
        append_gradient_polygon(
            &mut output,
            &band,
            from.offset(),
            to.offset(),
            from.color().to_premultiplied_array(),
            to.color().to_premultiplied_array(),
        );
        remaining = clip_gradient_polygon(&remaining, to.offset(), false);
    }

    let last = *stops.last().unwrap();
    append_gradient_polygon(
        &mut output,
        &remaining,
        last.offset(),
        last.offset(),
        last.color().to_premultiplied_array(),
        last.color().to_premultiplied_array(),
    );
    output
}

fn clip_gradient_polygon(
    polygon: &[GradientVertex],
    boundary: f32,
    keep_below: bool,
) -> Vec<GradientVertex> {
    let Some(mut previous) = polygon.last().copied() else {
        return Vec::new();
    };
    let mut previous_inside = if keep_below {
        previous.offset <= boundary
    } else {
        previous.offset >= boundary
    };
    let mut output = Vec::with_capacity(polygon.len() + 1);

    for &current in polygon {
        let current_inside = if keep_below {
            current.offset <= boundary
        } else {
            current.offset >= boundary
        };
        if previous_inside != current_inside {
            let amount = (boundary - previous.offset) / (current.offset - previous.offset);
            output.push(GradientVertex {
                point: Point::new(
                    previous.point.x + (current.point.x - previous.point.x) * amount,
                    previous.point.y + (current.point.y - previous.point.y) * amount,
                ),
                offset: boundary,
            });
        }
        if current_inside {
            output.push(current);
        }
        previous = current;
        previous_inside = current_inside;
    }
    output
}

fn append_gradient_polygon(
    output: &mut Vec<PaintedTriangle>,
    polygon: &[GradientVertex],
    start_offset: f32,
    end_offset: f32,
    start_color: [f32; 4],
    end_color: [f32; 4],
) {
    if polygon.len() < 3 {
        return;
    }
    for index in 1..polygon.len() - 1 {
        let vertices = [polygon[0], polygon[index], polygon[index + 1]];
        let twice_area = (vertices[1].point - vertices[0].point).x
            * (vertices[2].point - vertices[0].point).y
            - (vertices[1].point - vertices[0].point).y * (vertices[2].point - vertices[0].point).x;
        if twice_area == 0.0 {
            continue;
        }
        output.push(PaintedTriangle {
            points: vertices.map(|vertex| vertex.point),
            colors: vertices.map(|vertex| {
                let amount = if start_offset == end_offset {
                    0.0
                } else {
                    ((vertex.offset - start_offset) / (end_offset - start_offset)).clamp(0.0, 1.0)
                };
                std::array::from_fn(|component| {
                    start_color[component]
                        + (end_color[component] - start_color[component]) * amount
                })
            }),
        });
    }
}

fn evaluate_gradient_stops(stops: &[GradientStop], offset: f32) -> [f32; 4] {
    if offset <= stops[0].offset() {
        return stops[0].color().to_premultiplied_array();
    }
    for pair in stops.windows(2) {
        let [from, to] = [pair[0], pair[1]];
        if offset < to.offset() {
            let amount = (offset - from.offset()) / (to.offset() - from.offset());
            let from_color = from.color().to_premultiplied_array();
            let to_color = to.color().to_premultiplied_array();
            return std::array::from_fn(|component| {
                from_color[component] + (to_color[component] - from_color[component]) * amount
            });
        }
    }
    stops.last().unwrap().color().to_premultiplied_array()
}

fn paint_sampled_gradient(
    points: [Point; 3],
    sample: impl Fn(Point) -> [f32; 4] + Copy,
) -> Vec<PaintedTriangle> {
    let mut output = Vec::new();
    subdivide_sampled_gradient(points, sample, 0, &mut output);
    output
}

fn subdivide_sampled_gradient(
    points: [Point; 3],
    sample: impl Fn(Point) -> [f32; 4] + Copy,
    depth: u32,
    output: &mut Vec<PaintedTriangle>,
) {
    const MIN_DEPTH: u32 = 2;
    const MAX_DEPTH: u32 = 7;
    const COLOR_TOLERANCE: f32 = 1.0 / 255.0;

    let colors = points.map(sample);
    let midpoints = [
        midpoint(points[0], points[1]),
        midpoint(points[1], points[2]),
        midpoint(points[2], points[0]),
    ];
    let center = Point::new(
        (points[0].x + points[1].x + points[2].x) / 3.0,
        (points[0].y + points[1].y + points[2].y) / 3.0,
    );
    let mut error: f32 = 0.0;
    for edge in 0..3 {
        let actual = sample(midpoints[edge]);
        let expected = std::array::from_fn(|component| {
            (colors[edge][component] + colors[(edge + 1) % 3][component]) * 0.5
        });
        error = error.max(color_difference(actual, expected));
    }
    let actual_center = sample(center);
    let expected_center = std::array::from_fn(|component| {
        (colors[0][component] + colors[1][component] + colors[2][component]) / 3.0
    });
    error = error.max(color_difference(actual_center, expected_center));

    if depth >= MAX_DEPTH || (depth >= MIN_DEPTH && error <= COLOR_TOLERANCE) {
        output.push(PaintedTriangle { points, colors });
        return;
    }

    let [m01, m12, m20] = midpoints;
    for child in [
        [points[0], m01, m20],
        [m01, points[1], m12],
        [m20, m12, points[2]],
        [m01, m12, m20],
    ] {
        subdivide_sampled_gradient(child, sample, depth + 1, output);
    }
}

fn midpoint(first: Point, second: Point) -> Point {
    Point::new((first.x + second.x) * 0.5, (first.y + second.y) * 0.5)
}

fn color_difference(first: [f32; 4], second: [f32; 4]) -> f32 {
    first
        .into_iter()
        .zip(second)
        .map(|(first, second)| (first - second).abs())
        .fold(0.0, f32::max)
}

fn paint_mesh_gradient(points: [Point; 3], gradient: &MeshGradient) -> Vec<PaintedTriangle> {
    let mut output = Vec::new();
    for patch in gradient.triangles() {
        let patch_points = patch.map(GradientPoint::position);
        let orientation = signed_area(patch_points);
        let mut polygon = points.to_vec();
        for edge in 0..3 {
            polygon = clip_polygon_to_edge(
                &polygon,
                patch_points[edge],
                patch_points[(edge + 1) % 3],
                orientation,
            );
        }
        if polygon.len() < 3 {
            continue;
        }
        for index in 1..polygon.len() - 1 {
            let triangle_points = [polygon[0], polygon[index], polygon[index + 1]];
            if signed_area(triangle_points) == 0.0 {
                continue;
            }
            output.push(PaintedTriangle {
                points: triangle_points,
                colors: triangle_points.map(|point| mesh_color_at(point, *patch)),
            });
        }
    }
    output
}

fn clip_polygon_to_edge(
    polygon: &[Point],
    edge_start: Point,
    edge_end: Point,
    orientation: f32,
) -> Vec<Point> {
    let Some(mut previous) = polygon.last().copied() else {
        return Vec::new();
    };
    let mut previous_side = edge_side(edge_start, edge_end, previous);
    let mut previous_inside = previous_side * orientation >= 0.0;
    let mut output = Vec::with_capacity(polygon.len() + 1);
    for &current in polygon {
        let current_side = edge_side(edge_start, edge_end, current);
        let current_inside = current_side * orientation >= 0.0;
        if previous_inside != current_inside {
            let amount = previous_side / (previous_side - current_side);
            output.push(Point::new(
                previous.x + (current.x - previous.x) * amount,
                previous.y + (current.y - previous.y) * amount,
            ));
        }
        if current_inside {
            output.push(current);
        }
        previous = current;
        previous_side = current_side;
        previous_inside = current_inside;
    }
    output
}

fn mesh_color_at(point: Point, patch: [GradientPoint; 3]) -> [f32; 4] {
    let positions = patch.map(GradientPoint::position);
    let denominator = signed_area(positions);
    let weights = [
        edge_side(positions[1], positions[2], point) / denominator,
        edge_side(positions[2], positions[0], point) / denominator,
        edge_side(positions[0], positions[1], point) / denominator,
    ];
    let colors = patch.map(|vertex| vertex.color().to_premultiplied_array());
    std::array::from_fn(|component| {
        colors[0][component] * weights[0]
            + colors[1][component] * weights[1]
            + colors[2][component] * weights[2]
    })
}

fn signed_area(points: [Point; 3]) -> f32 {
    edge_side(points[0], points[1], points[2])
}

fn edge_side(start: Point, end: Point, point: Point) -> f32 {
    (end.x - start.x) * (point.y - start.y) - (end.y - start.y) * (point.x - start.x)
}

#[cfg(test)]
fn point_to_clip(point: Point, view: CanvasView) -> [f32; 4] {
    transformed_point_to_clip(point, view.canvas_to_view)
}

fn transformed_point_to_clip(point: Point, canvas_to_view: Projective2) -> [f32; 4] {
    let view_point = canvas_to_view.transform_homogeneous(point);
    [
        2.0 * view_point.x - view_point.z,
        view_point.z - 2.0 * view_point.y,
        0.0,
        view_point.z,
    ]
}

fn screen_space_tolerance(
    path: &Path,
    path_to_view: Projective2,
    viewport_width: f32,
    viewport_height: f32,
) -> f32 {
    const MIN_TOLERANCE: f32 = 1.0e-4;
    let mut maximum_scale = 0.0_f32;
    let points = path_reference_points(path);
    let points: &[Point] = if points.is_empty() {
        &[Point::new(0.0, 0.0)]
    } else {
        &points
    };

    for &point in points {
        let Some(scale) = local_pixel_scale(
            path_to_view,
            point,
            viewport_width.abs(),
            viewport_height.abs(),
        ) else {
            return MIN_TOLERANCE;
        };
        maximum_scale = maximum_scale.max(scale);
    }

    if maximum_scale == 0.0 {
        return DEFAULT_TESSELLATION_TOLERANCE;
    }
    let tolerance = DEFAULT_TESSELLATION_TOLERANCE / maximum_scale;
    if tolerance.is_finite() {
        tolerance.max(MIN_TOLERANCE)
    } else {
        MIN_TOLERANCE
    }
}

fn local_pixel_scale(
    transform: Projective2,
    point: Point,
    viewport_width: f32,
    viewport_height: f32,
) -> Option<f32> {
    let [[a, b, c], [d, e, f], [g, h, i]] = transform.rows();
    let numerator_x = a * point.x + b * point.y + c;
    let numerator_y = d * point.x + e * point.y + f;
    let denominator = g * point.x + h * point.y + i;
    if !denominator.is_finite() || denominator.abs() <= f32::EPSILON {
        return None;
    }
    let denominator_squared = denominator * denominator;
    let j00 = viewport_width * (a * denominator - g * numerator_x) / denominator_squared;
    let j01 = viewport_width * (b * denominator - h * numerator_x) / denominator_squared;
    let j10 = viewport_height * (d * denominator - g * numerator_y) / denominator_squared;
    let j11 = viewport_height * (e * denominator - h * numerator_y) / denominator_squared;
    if ![j00, j01, j10, j11].into_iter().all(f32::is_finite) {
        return None;
    }

    // Largest singular value of the 2x2 Jacobian: the greatest local stretch
    // from one Canvas unit to target pixels, independent of direction.
    let column0_squared = j00 * j00 + j10 * j10;
    let column1_squared = j01 * j01 + j11 * j11;
    let column_dot = j00 * j01 + j10 * j11;
    let discriminant =
        ((column0_squared - column1_squared).powi(2) + 4.0 * column_dot * column_dot).sqrt();
    let largest_eigenvalue = 0.5 * (column0_squared + column1_squared + discriminant);
    let scale = largest_eigenvalue.max(0.0).sqrt();
    scale.is_finite().then_some(scale)
}

fn path_reference_points(path: &Path) -> Vec<Point> {
    let mut points = Vec::new();
    for subpath in path.subpaths().iter().filter(|subpath| subpath.is_finite()) {
        let mut current = subpath.start();
        points.push(current);
        for segment in subpath.segments() {
            match *segment {
                PathSegment::LineTo(end) => points.push(end),
                PathSegment::QuadraticTo { control, end } => {
                    points.extend([control, end]);
                }
                PathSegment::CubicTo {
                    control1,
                    control2,
                    end,
                } => {
                    points.extend([control1, control2, end]);
                }
                PathSegment::EllipticalArcTo { radii, end, .. } => {
                    // SVG endpoint arcs may enlarge their supplied radii. This
                    // box deliberately overestimates the likely arc region so
                    // projective scale changes do not get missed completely.
                    let radius = radii
                        .x
                        .abs()
                        .max(radii.y.abs())
                        .max((end - current).length() * 0.5);
                    let center = Point::new((current.x + end.x) * 0.5, (current.y + end.y) * 0.5);
                    points.extend([
                        end,
                        Point::new(center.x - radius, center.y - radius),
                        Point::new(center.x + radius, center.y - radius),
                        Point::new(center.x + radius, center.y + radius),
                        Point::new(center.x - radius, center.y + radius),
                    ]);
                }
            }
            current = segment.end();
        }
    }
    points
}

fn target_region(viewport: Viewport, extent: vk::Extent2D) -> Option<(vk::Viewport, vk::Rect2D)> {
    if !viewport.has_finite_positive_size() {
        return None;
    }

    let left = viewport.x.max(0.0).floor().min(extent.width as f32) as i32;
    let top = viewport.y.max(0.0).floor().min(extent.height as f32) as i32;
    let right = (viewport.x + viewport.width)
        .max(0.0)
        .ceil()
        .min(extent.width as f32) as i32;
    let bottom = (viewport.y + viewport.height)
        .max(0.0)
        .ceil()
        .min(extent.height as f32) as i32;
    if right <= left || bottom <= top {
        return None;
    }

    Some((
        vk::Viewport {
            x: viewport.x,
            y: viewport.y,
            width: viewport.width,
            height: viewport.height,
            min_depth: 0.0,
            max_depth: 1.0,
        },
        vk::Rect2D {
            offset: vk::Offset2D { x: left, y: top },
            extent: vk::Extent2D {
                width: (right - left) as u32,
                height: (bottom - top) as u32,
            },
        },
    ))
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

fn preferred_sample_count(device: &RenderDevice, format: vk::Format) -> vk::SampleCountFlags {
    choose_sample_count(device.supported_color_sample_counts(format))
}

fn choose_sample_count(supported: vk::SampleCountFlags) -> vk::SampleCountFlags {
    if supported.contains(vk::SampleCountFlags::TYPE_4) {
        vk::SampleCountFlags::TYPE_4
    } else {
        vk::SampleCountFlags::TYPE_1
    }
}

unsafe fn create_pipelines(
    device: &ash::Device,
    color_format: vk::Format,
    sample_count: vk::SampleCountFlags,
    image_descriptor_set_layout: vk::DescriptorSetLayout,
) -> Result<Pipelines> {
    let triangle = unsafe {
        create_pipeline(
            device,
            color_format,
            TRIANGLE_VERTEX_SHADER,
            TRIANGLE_FRAGMENT_SHADER,
            size_of::<TrianglePushConstants>() as u32,
            &[],
            sample_count,
            CompositeOperator::SourceOver,
        )?
    };
    let image = match unsafe {
        create_pipeline(
            device,
            color_format,
            IMAGE_VERTEX_SHADER,
            IMAGE_FRAGMENT_SHADER,
            size_of::<ImagePushConstants>() as u32,
            &[image_descriptor_set_layout],
            sample_count,
            CompositeOperator::SourceOver,
        )
    } {
        Ok(image) => image,
        Err(error) => {
            unsafe {
                device.destroy_pipeline(triangle.handle, None);
                device.destroy_pipeline_layout(triangle.layout, None);
            }
            return Err(error);
        }
    };
    let source_over = match unsafe {
        create_pipeline(
            device,
            color_format,
            COMPOSITE_VERTEX_SHADER,
            COMPOSITE_FRAGMENT_SHADER,
            0,
            &[image_descriptor_set_layout],
            sample_count,
            CompositeOperator::SourceOver,
        )
    } {
        Ok(composite) => composite,
        Err(error) => {
            unsafe {
                for pipeline in [triangle, image] {
                    device.destroy_pipeline(pipeline.handle, None);
                    device.destroy_pipeline_layout(pipeline.layout, None);
                }
            }
            return Err(error);
        }
    };
    let mut composites = HashMap::from([(CompositeOperator::SourceOver, source_over)]);
    for operator in CompositeOperator::ALL {
        if operator == CompositeOperator::SourceOver {
            continue;
        }
        let composite = match unsafe {
            create_pipeline(
                device,
                color_format,
                COMPOSITE_VERTEX_SHADER,
                COMPOSITE_FRAGMENT_SHADER,
                0,
                &[image_descriptor_set_layout],
                sample_count,
                operator,
            )
        } {
            Ok(composite) => composite,
            Err(error) => {
                unsafe {
                    for pipeline in [triangle, image]
                        .into_iter()
                        .chain(composites.into_values())
                    {
                        device.destroy_pipeline(pipeline.handle, None);
                        device.destroy_pipeline_layout(pipeline.layout, None);
                    }
                }
                return Err(error);
            }
        };
        composites.insert(operator, composite);
    }
    let mask = match unsafe {
        create_pipeline(
            device,
            color_format,
            MASK_VERTEX_SHADER,
            MASK_FRAGMENT_SHADER,
            0,
            &[image_descriptor_set_layout],
            sample_count,
            CompositeOperator::DestinationIn,
        )
    } {
        Ok(mask) => mask,
        Err(error) => {
            unsafe {
                for pipeline in [triangle, image]
                    .into_iter()
                    .chain(composites.into_values())
                {
                    device.destroy_pipeline(pipeline.handle, None);
                    device.destroy_pipeline_layout(pipeline.layout, None);
                }
            }
            return Err(error);
        }
    };
    let present = match unsafe {
        create_pipeline(
            device,
            color_format,
            COMPOSITE_VERTEX_SHADER,
            COMPOSITE_FRAGMENT_SHADER,
            0,
            &[image_descriptor_set_layout],
            vk::SampleCountFlags::TYPE_1,
            CompositeOperator::SourceOver,
        )
    } {
        Ok(present) => present,
        Err(error) => {
            unsafe {
                for pipeline in [triangle, image, mask]
                    .into_iter()
                    .chain(composites.into_values())
                {
                    device.destroy_pipeline(pipeline.handle, None);
                    device.destroy_pipeline_layout(pipeline.layout, None);
                }
            }
            return Err(error);
        }
    };
    Ok(Pipelines {
        format: color_format,
        sample_count,
        triangle,
        image,
        composites,
        mask,
        present,
    })
}

fn composite_blend_factors(operator: CompositeOperator) -> (vk::BlendFactor, vk::BlendFactor) {
    match operator {
        CompositeOperator::Clear => (vk::BlendFactor::ZERO, vk::BlendFactor::ZERO),
        CompositeOperator::Copy => (vk::BlendFactor::ONE, vk::BlendFactor::ZERO),
        CompositeOperator::Destination => (vk::BlendFactor::ZERO, vk::BlendFactor::ONE),
        CompositeOperator::SourceOver => {
            (vk::BlendFactor::ONE, vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
        }
        CompositeOperator::DestinationOver => {
            (vk::BlendFactor::ONE_MINUS_DST_ALPHA, vk::BlendFactor::ONE)
        }
        CompositeOperator::SourceIn => (vk::BlendFactor::DST_ALPHA, vk::BlendFactor::ZERO),
        CompositeOperator::DestinationIn => (vk::BlendFactor::ZERO, vk::BlendFactor::SRC_ALPHA),
        CompositeOperator::SourceOut => {
            (vk::BlendFactor::ONE_MINUS_DST_ALPHA, vk::BlendFactor::ZERO)
        }
        CompositeOperator::DestinationOut => {
            (vk::BlendFactor::ZERO, vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
        }
        CompositeOperator::SourceAtop => (
            vk::BlendFactor::DST_ALPHA,
            vk::BlendFactor::ONE_MINUS_SRC_ALPHA,
        ),
        CompositeOperator::DestinationAtop => (
            vk::BlendFactor::ONE_MINUS_DST_ALPHA,
            vk::BlendFactor::SRC_ALPHA,
        ),
        CompositeOperator::Xor => (
            vk::BlendFactor::ONE_MINUS_DST_ALPHA,
            vk::BlendFactor::ONE_MINUS_SRC_ALPHA,
        ),
    }
}

#[allow(clippy::too_many_arguments)]
unsafe fn create_pipeline(
    device: &ash::Device,
    color_format: vk::Format,
    vertex_shader: &[u8],
    fragment_shader: &[u8],
    push_constant_size: u32,
    descriptor_set_layouts: &[vk::DescriptorSetLayout],
    sample_count: vk::SampleCountFlags,
    blend: CompositeOperator,
) -> Result<Pipeline> {
    let vertex_code = ash::util::read_spv(&mut Cursor::new(vertex_shader))?;
    let fragment_code = ash::util::read_spv(&mut Cursor::new(fragment_shader))?;
    let vertex_module = unsafe {
        device.create_shader_module(
            &vk::ShaderModuleCreateInfo::default().code(&vertex_code),
            None,
        )?
    };
    let fragment_module = unsafe {
        device.create_shader_module(
            &vk::ShaderModuleCreateInfo::default().code(&fragment_code),
            None,
        )?
    };

    let result = (|| {
        let vertex_entry: &CStr = c"vs_main";
        let fragment_entry: &CStr = c"fs_main";
        let stages = [
            vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::VERTEX)
                .module(vertex_module)
                .name(vertex_entry),
            vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::FRAGMENT)
                .module(fragment_module)
                .name(fragment_entry),
        ];
        let (src_color, dst_color) = composite_blend_factors(blend);
        let blend_attachment = vk::PipelineColorBlendAttachmentState::default()
            .blend_enable(true)
            .src_color_blend_factor(src_color)
            .dst_color_blend_factor(dst_color)
            .color_blend_op(vk::BlendOp::ADD)
            .src_alpha_blend_factor(src_color)
            .dst_alpha_blend_factor(dst_color)
            .alpha_blend_op(vk::BlendOp::ADD)
            .color_write_mask(
                vk::ColorComponentFlags::R
                    | vk::ColorComponentFlags::G
                    | vk::ColorComponentFlags::B
                    | vk::ColorComponentFlags::A,
            );
        let push_range = vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::VERTEX)
            .size(push_constant_size);
        let push_ranges = if push_constant_size == 0 {
            &[][..]
        } else {
            std::slice::from_ref(&push_range)
        };
        let layout = unsafe {
            device.create_pipeline_layout(
                &vk::PipelineLayoutCreateInfo::default()
                    .set_layouts(descriptor_set_layouts)
                    .push_constant_ranges(push_ranges),
                None,
            )?
        };
        let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
        let vertex_input = vk::PipelineVertexInputStateCreateInfo::default();
        let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
            .topology(vk::PrimitiveTopology::TRIANGLE_LIST);
        let viewport = vk::PipelineViewportStateCreateInfo::default()
            .viewport_count(1)
            .scissor_count(1);
        let rasterization = vk::PipelineRasterizationStateCreateInfo::default()
            .polygon_mode(vk::PolygonMode::FILL)
            .cull_mode(vk::CullModeFlags::NONE)
            .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
            .line_width(1.0);
        let multisample =
            vk::PipelineMultisampleStateCreateInfo::default().rasterization_samples(sample_count);
        let color_blend = vk::PipelineColorBlendStateCreateInfo::default()
            .attachments(std::slice::from_ref(&blend_attachment));
        let dynamic = vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);
        let color_formats = [color_format];
        let mut rendering_info =
            vk::PipelineRenderingCreateInfo::default().color_attachment_formats(&color_formats);
        let info = vk::GraphicsPipelineCreateInfo::default()
            .stages(&stages)
            .vertex_input_state(&vertex_input)
            .input_assembly_state(&input_assembly)
            .viewport_state(&viewport)
            .rasterization_state(&rasterization)
            .multisample_state(&multisample)
            .color_blend_state(&color_blend)
            .dynamic_state(&dynamic)
            .layout(layout)
            .render_pass(vk::RenderPass::null())
            .push_next(&mut rendering_info);
        let pipeline = unsafe {
            device
                .create_graphics_pipelines(vk::PipelineCache::null(), &[info], None)
                .map_err(|(_, error)| error)?[0]
        };
        Ok::<_, Box<dyn Error>>(Pipeline {
            layout,
            handle: pipeline,
        })
    })();
    unsafe {
        device.destroy_shader_module(fragment_module, None);
        device.destroy_shader_module(vertex_module, None);
    }
    result
}

#[allow(clippy::too_many_arguments)]
unsafe fn transition_image(
    device: &ash::Device,
    command_buffer: vk::CommandBuffer,
    image: vk::Image,
    old_layout: vk::ImageLayout,
    new_layout: vk::ImageLayout,
    src_stage_mask: vk::PipelineStageFlags2,
    src_access_mask: vk::AccessFlags2,
    dst_stage_mask: vk::PipelineStageFlags2,
    dst_access_mask: vk::AccessFlags2,
) {
    let barrier = vk::ImageMemoryBarrier2::default()
        .src_stage_mask(src_stage_mask)
        .src_access_mask(src_access_mask)
        .dst_stage_mask(dst_stage_mask)
        .dst_access_mask(dst_access_mask)
        .old_layout(old_layout)
        .new_layout(new_layout)
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .image(image)
        .subresource_range(
            vk::ImageSubresourceRange::default()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .level_count(1)
                .layer_count(1),
        );
    let dependency =
        vk::DependencyInfo::default().image_memory_barriers(std::slice::from_ref(&barrier));
    unsafe { device.cmd_pipeline_barrier2(command_buffer, &dependency) };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::{Color, ConicGradient, GradientPoint, GradientStop, RadialGradient};
    use crate::path::{Path, Subpath};

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
    fn path_tolerance_tracks_target_pixel_scale() {
        let path = Path::polyline([Point::new(0.0, 0.0), Point::new(1.0, 1.0)]);
        let identity = screen_space_tolerance(&path, Projective2::IDENTITY, 100.0, 100.0);
        let enlarged = screen_space_tolerance(
            &path,
            Projective2::from_rows([[10.0, 0.0, 0.0], [0.0, 10.0, 0.0], [0.0, 0.0, 1.0]]),
            100.0,
            100.0,
        );
        let reduced = screen_space_tolerance(
            &path,
            Projective2::from_rows([[0.1, 0.0, 0.0], [0.0, 0.1, 0.0], [0.0, 0.0, 1.0]]),
            100.0,
            100.0,
        );

        assert!((identity - DEFAULT_TESSELLATION_TOLERANCE / 100.0).abs() < 1.0e-6);
        assert!((enlarged - identity / 10.0).abs() < 1.0e-6);
        assert!((reduced - identity * 10.0).abs() < 1.0e-6);
    }

    #[test]
    fn only_coverage_boundaries_reserve_independent_results() {
        let masked = Canvas::draw(Canvas::empty(), Coverage::from_alpha(Canvas::empty()))
            .transformed(Projective2::IDENTITY);
        let root = Canvas::empty().composite(masked, CompositeOperator::SourceOver);

        assert_eq!(required_render_images(&root), 3);
    }

    #[test]
    fn non_source_over_composites_create_group_boundaries() {
        let leaf = Canvas::draw(Color::rgb8(255, 255, 255), Coverage::all());
        let grouped = leaf.clone().composite(leaf, CompositeOperator::SourceIn);
        let root = Canvas::empty().composite(grouped.clone(), CompositeOperator::SourceOver);

        assert!(!can_render_source_over_directly(&grouped));
        assert_eq!(required_render_images(&grouped), 2);
        assert_eq!(required_render_images(&root), 3);
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
            (CompositeOperator::Clear, (zero, zero)),
            (CompositeOperator::Copy, (one, zero)),
            (CompositeOperator::Destination, (zero, one)),
            (CompositeOperator::SourceOver, (one, one_minus_source_alpha)),
            (
                CompositeOperator::DestinationOver,
                (one_minus_destination_alpha, one),
            ),
            (CompositeOperator::SourceIn, (destination_alpha, zero)),
            (CompositeOperator::DestinationIn, (zero, source_alpha)),
            (
                CompositeOperator::SourceOut,
                (one_minus_destination_alpha, zero),
            ),
            (
                CompositeOperator::DestinationOut,
                (zero, one_minus_source_alpha),
            ),
            (
                CompositeOperator::SourceAtop,
                (destination_alpha, one_minus_source_alpha),
            ),
            (
                CompositeOperator::DestinationAtop,
                (one_minus_destination_alpha, source_alpha),
            ),
            (
                CompositeOperator::Xor,
                (one_minus_destination_alpha, one_minus_source_alpha),
            ),
        ];

        for (operator, factors) in expected {
            assert_eq!(composite_blend_factors(operator), factors);
        }
    }

    #[test]
    fn along_path_offsets_restart_for_each_subpath() {
        let path = Path::from_subpaths([
            Subpath::new(Point::new(0.0, 0.0)).line_to(Point::new(10.0, 0.0)),
            Subpath::new(Point::new(0.0, 20.0)).line_to(Point::new(100.0, 20.0)),
        ]);
        let sampler = PathSampler::new(&path, DEFAULT_TESSELLATION_TOLERANCE);

        assert!((sampler.nearest_offset(Point::new(2.5, 1.0)).unwrap() - 0.25).abs() < 0.001);
        assert!((sampler.nearest_offset(Point::new(25.0, 19.0)).unwrap() - 0.25).abs() < 0.001);
        assert!(
            (sampler
                .nearest_sample(Point::new(2.5, 1.0))
                .unwrap()
                .signed_distance
                - 1.0)
                .abs()
                < 0.001
        );
        assert!(
            (sampler
                .nearest_sample(Point::new(25.0, 19.0))
                .unwrap()
                .signed_distance
                + 1.0)
                .abs()
                < 0.001
        );
    }

    #[test]
    fn converts_canvas_corners_to_clip_space() {
        let view =
            CanvasView::axis_aligned(Point::new(0.0, 0.0), Point::new(800.0, 600.0)).unwrap();
        assert_eq!(
            point_to_clip(Point::new(0.0, 0.0), view),
            [-1.0, 1.0, 0.0, 1.0]
        );
        assert_eq!(
            point_to_clip(Point::new(800.0, 600.0), view),
            [1.0, -1.0, 0.0, 1.0]
        );
    }

    #[test]
    fn applies_view_transform_before_projection() {
        let view =
            CanvasView::axis_aligned(Point::new(100.0, 50.0), Point::new(200.0, 100.0)).unwrap();
        assert_eq!(
            point_to_clip(Point::new(100.0, 50.0), view),
            [-1.0, 1.0, 0.0, 1.0]
        );
        assert_eq!(
            point_to_clip(Point::new(200.0, 100.0), view),
            [1.0, -1.0, 0.0, 1.0]
        );
    }

    #[test]
    fn preserves_projective_w_until_the_gpu_divide() {
        let quad = [
            Point::new(-2.0, -1.0),
            Point::new(2.0, -1.0),
            Point::new(1.0, 1.0),
            Point::new(-1.0, 1.0),
        ];
        let [p00, p10, p11, p01] = quad;
        let view = CanvasView::projective(p00, p10, p11, p01).unwrap();
        let expected_ndc = [[-1.0, 1.0], [1.0, 1.0], [1.0, -1.0], [-1.0, -1.0]];

        let clip_positions = quad.map(|point| point_to_clip(point, view));
        for (clip, expected) in clip_positions.into_iter().zip(expected_ndc) {
            assert!((clip[0] / clip[3] - expected[0]).abs() < 0.001);
            assert!((clip[1] / clip[3] - expected[1]).abs() < 0.001);
        }
        assert!(
            clip_positions
                .windows(2)
                .any(|pair| (pair[0][3] - pair[1][3]).abs() > 0.001)
        );
    }

    #[test]
    fn linear_gradient_splits_triangles_at_stop_boundaries() {
        let green = Color::rgb8(0, 255, 0).to_premultiplied_array();
        let gradient = LinearGradient::new(
            Point::new(0.0, 0.0),
            Point::new(10.0, 0.0),
            [
                GradientStop::new(0.0, Color::rgb8(255, 0, 0)).unwrap(),
                GradientStop::new(0.5, Color::rgb8(0, 255, 0)).unwrap(),
                GradientStop::new(1.0, Color::rgb8(0, 0, 255)).unwrap(),
            ],
        )
        .unwrap();
        let triangles = paint_triangle(
            [
                Point::new(0.0, 0.0),
                Point::new(10.0, 0.0),
                Point::new(0.0, 10.0),
            ],
            &Source::LinearGradient(gradient),
        );

        assert!(triangles.len() > 1);
        assert!(
            triangles
                .iter()
                .flat_map(|triangle| triangle.colors)
                .any(|color| color == green)
        );
        for triangle in triangles {
            let min = triangle
                .points
                .iter()
                .map(|point| point.x / 10.0)
                .fold(f32::INFINITY, f32::min);
            let max = triangle
                .points
                .iter()
                .map(|point| point.x / 10.0)
                .fold(f32::NEG_INFINITY, f32::max);
            assert!(!(min < 0.5 - 0.0001 && max > 0.5 + 0.0001));
        }
    }

    #[test]
    fn linear_gradient_clamps_colors_outside_its_axis() {
        let red = Color::rgb8(255, 0, 0).to_premultiplied_array();
        let gradient = LinearGradient::new(
            Point::new(0.0, 0.0),
            Point::new(10.0, 0.0),
            [
                GradientStop::new(0.2, Color::rgb8(255, 0, 0)).unwrap(),
                GradientStop::new(0.8, Color::rgb8(0, 0, 255)).unwrap(),
            ],
        )
        .unwrap();
        let triangles = paint_triangle(
            [
                Point::new(-10.0, 0.0),
                Point::new(-2.0, 0.0),
                Point::new(-10.0, 8.0),
            ],
            &Source::LinearGradient(gradient),
        );

        assert_eq!(triangles.len(), 1);
        assert_eq!(triangles[0].colors, [red; 3]);
    }

    #[test]
    fn triangular_mesh_gradient_reproduces_its_vertex_colors() {
        let positions = [
            Point::new(0.0, 0.0),
            Point::new(10.0, 0.0),
            Point::new(0.0, 10.0),
        ];
        let colors = [
            Color::rgb8(255, 0, 0),
            Color::rgb8(0, 255, 0),
            Color::rgb8(0, 0, 255),
        ];
        let gradient = MeshGradient::from_triangle(std::array::from_fn(|index| {
            GradientPoint::new(positions[index], colors[index])
        }))
        .unwrap();
        let triangles = paint_triangle(positions, &Source::MeshGradient(gradient));

        assert_eq!(triangles.len(), 1);
        for (point, color) in positions.into_iter().zip(colors) {
            let index = triangles[0]
                .points
                .iter()
                .position(|candidate| *candidate == point)
                .unwrap();
            assert_eq!(triangles[0].colors[index], color.to_premultiplied_array());
        }
    }

    #[test]
    fn mesh_gradient_clips_painted_geometry_to_its_patch() {
        let patch = [
            GradientPoint::new(Point::new(0.0, 0.0), Color::rgb8(255, 0, 0)),
            GradientPoint::new(Point::new(4.0, 0.0), Color::rgb8(0, 255, 0)),
            GradientPoint::new(Point::new(0.0, 4.0), Color::rgb8(0, 0, 255)),
        ];
        let gradient = MeshGradient::from_triangle(patch).unwrap();
        let triangles = paint_triangle(
            [
                Point::new(-10.0, -10.0),
                Point::new(20.0, -10.0),
                Point::new(-10.0, 20.0),
            ],
            &Source::MeshGradient(gradient),
        );

        assert!(!triangles.is_empty());
        assert!(
            triangles
                .iter()
                .flat_map(|triangle| triangle.points)
                .all(|point| point.x >= -0.0001
                    && point.y >= -0.0001
                    && point.x + point.y <= 4.0001)
        );
    }

    #[test]
    fn radial_gradient_samples_center_and_radius_colors() {
        let red = Color::rgb8(255, 0, 0);
        let blue = Color::rgb8(0, 0, 255);
        let gradient = RadialGradient::new(
            Point::new(0.0, 0.0),
            10.0,
            [
                GradientStop::new(0.0, red).unwrap(),
                GradientStop::new(1.0, blue).unwrap(),
            ],
        )
        .unwrap();
        let triangles = paint_triangle(
            [
                Point::new(0.0, 0.0),
                Point::new(10.0, 0.0),
                Point::new(0.0, 10.0),
            ],
            &Source::RadialGradient(gradient),
        );

        assert!(triangles.len() > 16);
        assert!(triangles.iter().any(|triangle| {
            triangle
                .points
                .iter()
                .zip(triangle.colors)
                .any(|(point, color)| {
                    *point == Point::new(0.0, 0.0) && color == red.to_premultiplied_array()
                })
        }));
        assert!(triangles.iter().any(|triangle| {
            triangle
                .points
                .iter()
                .zip(triangle.colors)
                .any(|(point, color)| {
                    *point == Point::new(10.0, 0.0) && color == blue.to_premultiplied_array()
                })
        }));
    }

    #[test]
    fn conic_gradient_uses_positive_parameter_angle() {
        let red = Color::rgb8(255, 0, 0);
        let green = Color::rgb8(0, 255, 0);
        let blue = Color::rgb8(0, 0, 255);
        let gradient = ConicGradient::new(
            Point::new(0.0, 0.0),
            0.0,
            [
                GradientStop::new(0.0, red).unwrap(),
                GradientStop::new(0.5, green).unwrap(),
                GradientStop::new(1.0, blue).unwrap(),
            ],
        )
        .unwrap();
        let triangles = paint_triangle(
            [
                Point::new(10.0, 0.0),
                Point::new(-10.0, 0.0),
                Point::new(0.0, 10.0),
            ],
            &Source::ConicGradient(gradient),
        );

        assert!(triangles.len() > 16);
        assert!(triangles.iter().any(|triangle| {
            triangle
                .points
                .iter()
                .zip(triangle.colors)
                .any(|(point, color)| {
                    *point == Point::new(10.0, 0.0) && color == red.to_premultiplied_array()
                })
        }));
        assert!(triangles.iter().any(|triangle| {
            triangle
                .points
                .iter()
                .zip(triangle.colors)
                .any(|(point, color)| {
                    *point == Point::new(-10.0, 0.0) && color == green.to_premultiplied_array()
                })
        }));
    }
}
