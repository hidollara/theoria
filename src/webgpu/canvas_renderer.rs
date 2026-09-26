use wgpu::util::DeviceExt;

use crate::{
    canvas::{
        Canvas, CanvasColor, CanvasNode, CanvasView, ColorField, Composite, Composition, Coverage,
        CoverageNode, Draw, ImageExtend, ImagePattern, ImageSampling,
        canvas_image::CanvasImage,
        image::{Image, ImageKind},
    },
    canvas_renderer::{
        canvas_plan::{
            can_record_directly_in, collect_blend_shaders_in, collect_images, prepare_colors_in,
            required_scratch_images_in,
        },
        color_space::PreparedColors,
        mesh_paint::paint_triangle_prepared,
        output_color::OutputColorTransform,
        path_sampling::PathSampler,
        view_space::screen_space_tolerance,
    },
    color::{Color, IccProfile, IccRenderingIntent, RgbColorSpace},
    geometry::Point,
    path::Path,
    surface::{SurfaceSize, Viewport},
    tessellation::{
        DEFAULT_TESSELLATION_TOLERANCE, Mesh, tessellate_fill_with_tolerance,
        tessellate_stroke_with_tolerance,
    },
    transform::Projective2,
};

use super::color::PremultipliedWorkingRgba;

mod encoding;
mod image;
mod pipeline;

use encoding::{CompositionUniform, ImageVertex, TriangleVertex, bytes_of, bytes_of_slice};
use image::{GpuImage, GpuImageColor, ImageCache, create_working_image};
use pipeline::{Pipelines, WORKING_FORMAT};

/// A WebGPU texture region that can receive one Canvas evaluation.
///
/// `format` and `size` must describe `view`. A zero-sized target is ignored.
pub struct RenderTarget<'a> {
    view: &'a wgpu::TextureView,
    format: wgpu::TextureFormat,
    size: SurfaceSize,
    clear_color: CanvasColor,
    output_profile: Option<(&'a IccProfile, IccRenderingIntent)>,
    preserve_working: bool,
}

impl<'a> RenderTarget<'a> {
    pub fn new(
        view: &'a wgpu::TextureView,
        format: wgpu::TextureFormat,
        size: SurfaceSize,
        clear_color: impl Into<CanvasColor>,
    ) -> Self {
        Self {
            view,
            format,
            size,
            clear_color: clear_color.into(),
            output_profile: None,
            preserve_working: false,
        }
    }

    /// Encodes the final working image into ICC device RGB coordinates.
    /// Use an unsigned-normalized, non-sRGB target format so the GPU does not
    /// apply a second sRGB transfer curve to the profile's encoded values.
    pub fn with_icc_output(mut self, profile: &'a IccProfile, intent: IccRenderingIntent) -> Self {
        self.output_profile = Some((profile, intent));
        self
    }

    fn preserving_working(mut self) -> Self {
        self.preserve_working = true;
        self
    }
}

/// Translates backend-independent Canvas content into WebGPU commands.
///
/// The renderer owns cheap handles to the supplied device and queue, caches
/// uploaded Canvas images, and submits one or more command buffers from
/// [`render`](Self::render). It does not create or present a WebGPU surface.
pub struct CanvasRenderer {
    working_space: RgbColorSpace,
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipelines: Pipelines,
    images: ImageCache,
    scratch_images: Vec<GpuImage>,
    scratch_size: Option<SurfaceSize>,
    composite_sampler: wgpu::Sampler,
}

impl CanvasRenderer {
    pub fn new(device: wgpu::Device, queue: wgpu::Queue) -> Self {
        Self::new_with_working_space(device, queue, RgbColorSpace::LinearSrgb)
    }

    /// Selects the RGB coordinates used for intermediate Canvas evaluation.
    /// The choice is fixed for this renderer so cached Canvas images keep one meaning.
    pub fn new_with_working_space(
        device: wgpu::Device,
        queue: wgpu::Queue,
        working_space: RgbColorSpace,
    ) -> Self {
        let pipelines = Pipelines::new(&device, working_space);
        let composite_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("theoria WebGPU Canvas sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Self {
            working_space,
            device,
            queue,
            pipelines,
            images: ImageCache::new(),
            scratch_images: Vec::new(),
            scratch_size: None,
            composite_sampler,
        }
    }

    pub fn render(
        &mut self,
        target: RenderTarget<'_>,
        viewport: Viewport,
        canvas: &Canvas,
        view: CanvasView,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if target.size.width == 0 || target.size.height == 0 {
            return Ok(());
        }
        let prepared = prepare_colors_in(canvas, self.working_space)?;
        target
            .clear_color
            .try_straight_rgba_in(self.working_space)?;
        self.materialize_canvas_resources(canvas)?;
        self.pipelines
            .retain_icc_images(&self.images.active_icc_keys());
        self.render_to_view(target, viewport, canvas, view, &prepared)?;
        Ok(())
    }

    fn materialize_canvas_resources(
        &mut self,
        canvas: &Canvas,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut images = Vec::new();
        collect_images(canvas, &mut images);
        for image in images {
            self.materialize_image(&image)?;
        }
        Ok(())
    }

    fn materialize_image(&mut self, image: &Image) -> Result<(), Box<dyn std::error::Error>> {
        match image.kind() {
            ImageKind::Raster(image) => {
                if let crate::canvas::image::RasterImageView::Icc8 {
                    profile,
                    intent,
                    has_alpha,
                    ..
                } = image.view()
                {
                    self.pipelines.ensure_icc_image(
                        &self.device,
                        profile,
                        intent,
                        has_alpha,
                        self.working_space,
                    )?;
                }
                self.images
                    .ensure_uploaded(&self.device, &self.queue, image.as_ref())?;
            }
            ImageKind::Canvas(image) => self.materialize_canvas_image(image)?,
        }
        Ok(())
    }

    fn materialize_canvas_image(
        &mut self,
        image: &CanvasImage,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if self.images.contains_canvas(image) {
            return Ok(());
        }
        let prepared = prepare_colors_in(image.content(), self.working_space)?;
        self.materialize_canvas_resources(image.content())?;
        let backing = create_working_image(
            &self.device,
            image.width(),
            image.height(),
            "theoria WebGPU Canvas image",
            self.working_space,
        );
        self.render_to_view(
            RenderTarget::new(
                &backing.view,
                WORKING_FORMAT,
                SurfaceSize {
                    width: image.width(),
                    height: image.height(),
                },
                Color::rgba8(0, 0, 0, 0),
            )
            .preserving_working(),
            Viewport::new(0.0, 0.0, image.width() as f32, image.height() as f32),
            image.content(),
            image.view(),
            &prepared,
        )?;
        self.images.insert_canvas(image, backing);
        Ok(())
    }

    fn render_to_view(
        &mut self,
        target: RenderTarget<'_>,
        viewport: Viewport,
        canvas: &Canvas,
        view: CanvasView,
        prepared: &PreparedColors,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let output = target
            .output_profile
            .map(|(profile, intent)| OutputColorTransform::new(profile, intent, self.working_space))
            .transpose()?;
        if output.is_some()
            && matches!(
                target.format,
                wgpu::TextureFormat::Rgba8UnormSrgb | wgpu::TextureFormat::Bgra8UnormSrgb
            )
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "ICC output requires a non-sRGB render target format",
            )
            .into());
        }
        let mut blend_shaders = Vec::new();
        collect_blend_shaders_in(canvas, &mut blend_shaders, self.working_space);
        self.pipelines
            .ensure_blend_shaders(&self.device, &blend_shaders);
        self.ensure_scratch_images(
            target.size,
            required_scratch_images_in(canvas, self.working_space),
        );
        let target_region = target_region(viewport, target.size);
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("theoria WebGPU Canvas commands"),
            });
        let source = {
            let mut recorder = CanvasRecorder {
                device: &self.device,
                pipelines: &self.pipelines,
                images: &self.images,
                scratch_images: &self.scratch_images,
                composite_sampler: &self.composite_sampler,
                encoder: &mut encoder,
                size: target.size,
                target_region,
                view,
                prepared,
                next_image: 0,
            };
            recorder.evaluate(canvas, Projective2::IDENTITY)
        };

        let source_binding = sampled_binding(
            &self.device,
            &self.pipelines.texture_layout,
            &self.composite_sampler,
            &self.scratch_images[source].view,
            "theoria WebGPU present source",
        );
        let present = match (target.output_profile, &output) {
            (Some((profile, intent)), Some(transform)) => self.pipelines.icc_present(
                &self.device,
                target.format,
                profile,
                intent,
                transform,
            )?,
            _ if target.preserve_working => &self.pipelines.composites[&Composite::SourceOver],
            _ => self.pipelines.present(&self.device, target.format),
        };
        let clear = crate::canvas_renderer::color_space::lower_straight_color_in(
            &target.clear_color,
            self.working_space,
        );
        let clear = match &output {
            Some(transform) => transform.convert_premultiplied(clear)?,
            None if target.preserve_working => clear,
            None => crate::canvas_renderer::color_space::convert_premultiplied_space(
                clear,
                self.working_space,
                RgbColorSpace::LinearSrgb,
            ),
        };
        let attachment = Some(wgpu::RenderPassColorAttachment {
            view: target.view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color {
                    r: f64::from(clear[0]),
                    g: f64::from(clear[1]),
                    b: f64::from(clear[2]),
                    a: f64::from(clear[3]),
                }),
                store: wgpu::StoreOp::Store,
            },
        });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("theoria WebGPU present"),
            color_attachments: &[attachment],
            ..Default::default()
        });
        pass.set_pipeline(present);
        pass.set_bind_group(0, &source_binding, &[]);
        pass.draw(0..3, 0..1);
        drop(pass);

        self.queue.submit([encoder.finish()]);
        Ok(())
    }

    fn ensure_scratch_images(&mut self, size: SurfaceSize, required: usize) {
        if self.scratch_size != Some(size) {
            self.scratch_images.clear();
            self.scratch_size = Some(size);
        }
        while self.scratch_images.len() < required {
            self.scratch_images.push(create_working_image(
                &self.device,
                size.width,
                size.height,
                "theoria WebGPU Canvas scratch image",
                self.working_space,
            ));
        }
    }
}

struct CanvasRecorder<'a> {
    device: &'a wgpu::Device,
    pipelines: &'a Pipelines,
    images: &'a ImageCache,
    scratch_images: &'a [GpuImage],
    composite_sampler: &'a wgpu::Sampler,
    encoder: &'a mut wgpu::CommandEncoder,
    size: SurfaceSize,
    target_region: Option<TargetRegion>,
    view: CanvasView,
    prepared: &'a PreparedColors,
    next_image: usize,
}

impl CanvasRecorder<'_> {
    fn evaluate(&mut self, canvas: &Canvas, canvas_to_root: Projective2) -> usize {
        let accumulator = self.allocate_image();
        self.clear(accumulator);
        self.render_node(accumulator, canvas, canvas_to_root);
        accumulator
    }

    fn allocate_image(&mut self) -> usize {
        let image = self.next_image;
        self.next_image += 1;
        debug_assert!(image < self.scratch_images.len());
        image
    }

    fn render_node(&mut self, accumulator: usize, canvas: &Canvas, canvas_to_root: Projective2) {
        match canvas.node() {
            CanvasNode::Draw(draw) => self.render_draw(accumulator, draw, canvas_to_root),
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
                    && composition.composite_space() == self.prepared.working_space()
                    && can_record_directly_in(source, self.prepared.working_space())
                {
                    self.render_node(accumulator, source, canvas_to_root);
                } else {
                    let source_image = self.evaluate(source, canvas_to_root);
                    if !composition.requires_destination_read_in(self.prepared.working_space()) {
                        self.draw_composite(accumulator, source_image, composition.composite());
                    } else {
                        self.compose_with_destination(accumulator, source_image, composition);
                    }
                }
            }
        }
    }

    fn render_draw(&mut self, accumulator: usize, draw: &Draw, draw_to_root: Projective2) {
        if self.target_region.is_none() {
            return;
        }
        if let ColorField::Canvas(content) = &draw.source {
            self.render_canvas_source(accumulator, content, &draw.coverage, draw_to_root);
            return;
        }
        match draw.coverage.node() {
            CoverageNode::All => {
                self.render_unbounded_color_field(accumulator, &draw.source, draw_to_root)
            }
            CoverageNode::Fill { path, rule } => {
                let tolerance = self.path_tolerance(path, draw_to_root);
                let mesh = tessellate_fill_with_tolerance(path, *rule, tolerance);
                self.render_mesh(accumulator, &mesh, &draw.source, draw_to_root);
            }
            CoverageNode::Stroke { path, style } => {
                let tolerance = self.path_tolerance(path, draw_to_root);
                let mesh = tessellate_stroke_with_tolerance(path, style, tolerance);
                self.render_mesh(accumulator, &mesh, &draw.source, draw_to_root);
            }
            CoverageNode::AlphaOf(coverage_source) => {
                let content = self.evaluate_color_field(&draw.source, draw_to_root);
                let mask = self.evaluate_color_field(coverage_source, draw_to_root);
                self.apply_mask(content, mask);
                self.draw_composite(accumulator, content, Composite::SourceOver);
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
        let content_image = self.evaluate(content, draw_to_root);
        if let Some(mask) = self.evaluate_coverage(coverage, draw_to_root) {
            self.apply_mask(content_image, mask);
        }
        self.draw_composite(accumulator, content_image, Composite::SourceOver);
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

    fn evaluate_color_field(&mut self, source: &ColorField, source_to_root: Projective2) -> usize {
        if let ColorField::Canvas(canvas) = source {
            return self.evaluate(canvas, source_to_root);
        }
        let image = self.allocate_image();
        self.clear(image);
        self.render_unbounded_color_field(image, source, source_to_root);
        image
    }

    fn evaluate_geometry_mask(&mut self, mesh: &Mesh, mask_to_root: Projective2) -> usize {
        let image = self.allocate_image();
        self.clear(image);
        self.render_mesh(
            image,
            mesh,
            &ColorField::Solid(Color::rgb8(255, 255, 255).into()),
            mask_to_root,
        );
        image
    }

    fn compose_with_destination(
        &mut self,
        accumulator: usize,
        source: usize,
        composition: &Composition,
    ) {
        let destination = self.allocate_image();
        self.clear(destination);
        self.draw_composite(destination, accumulator, Composite::Copy);
        self.clear(accumulator);

        let destination_binding =
            self.scratch_binding(destination, "theoria WebGPU blend backdrop");
        let source_binding = self.scratch_binding(source, "theoria WebGPU blend source");
        let operation = CompositionUniform::new(composition);
        let uniform = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("theoria WebGPU blend operation"),
                contents: bytes_of(&operation),
                usage: wgpu::BufferUsages::UNIFORM,
            });
        let operation_binding = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("theoria WebGPU blend operation"),
            layout: &self.pipelines.blend_uniform_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            }],
        });
        let attachment = load_attachment(&self.scratch_images[accumulator].view);
        let mut pass = self.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("theoria WebGPU blend composite"),
            color_attachments: &[Some(attachment)],
            ..Default::default()
        });
        set_full_region(&mut pass, self.size);
        pass.set_pipeline(self.pipelines.blend(composition.blend_shader()));
        pass.set_bind_group(0, &destination_binding, &[]);
        pass.set_bind_group(1, &source_binding, &[]);
        pass.set_bind_group(2, &operation_binding, &[]);
        pass.draw(0..3, 0..1);
    }

    fn render_unbounded_color_field(
        &mut self,
        accumulator: usize,
        source: &ColorField,
        source_to_root: Projective2,
    ) {
        if let Some(mesh) = self.unbounded_mesh(source_to_root) {
            self.render_mesh(accumulator, &mesh, source, source_to_root);
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

    fn render_mesh(
        &mut self,
        accumulator: usize,
        mesh: &Mesh,
        source: &ColorField,
        mesh_to_root: Projective2,
    ) {
        if let ColorField::Image(pattern) = source {
            self.render_image_mesh(accumulator, mesh, pattern, mesh_to_root);
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
        let mut vertices = Vec::new();
        for triangle in mesh.indices.chunks_exact(3) {
            let points = [
                mesh.vertices[triangle[0] as usize],
                mesh.vertices[triangle[1] as usize],
                mesh.vertices[triangle[2] as usize],
            ];
            for painted in paint_triangle_prepared::<PremultipliedWorkingRgba>(
                points,
                source,
                path_sampler.as_ref(),
                self.prepared,
            ) {
                vertices.extend(TriangleVertex::encode(
                    painted.points,
                    painted.colors,
                    mesh_to_root,
                    self.view,
                ));
            }
        }
        if vertices.is_empty() {
            return;
        }
        let buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("theoria WebGPU Canvas triangles"),
                contents: bytes_of_slice(&vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
        let region = self
            .target_region
            .expect("Canvas drawing is skipped for an empty target region");
        let attachment = load_attachment(&self.scratch_images[accumulator].view);
        let mut pass = self.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("theoria WebGPU Canvas triangles"),
            color_attachments: &[Some(attachment)],
            ..Default::default()
        });
        set_canvas_region(&mut pass, region);
        pass.set_pipeline(&self.pipelines.triangle);
        pass.set_vertex_buffer(0, buffer.slice(..));
        pass.draw(0..vertices.len() as u32, 0..1);
    }

    fn render_image_mesh(
        &mut self,
        accumulator: usize,
        mesh: &Mesh,
        pattern: &ImagePattern,
        mesh_to_root: Projective2,
    ) {
        let image = self
            .images
            .get(pattern.image())
            .expect("Canvas images are materialized before command recording");
        let (premultiplied, space, icc_pipeline) = match &image.color {
            GpuImageColor::Rgb {
                premultiplied,
                space,
            } => (*premultiplied, *space, None),
            GpuImageColor::Icc { key } => (
                false,
                crate::color::RgbColorSpace::LinearSrgb,
                Some(self.pipelines.icc_image(key)),
            ),
        };
        let mut vertices = Vec::with_capacity(mesh.indices.len());
        for triangle in mesh.indices.chunks_exact(3) {
            let points = [
                mesh.vertices[triangle[0] as usize],
                mesh.vertices[triangle[1] as usize],
                mesh.vertices[triangle[2] as usize],
            ];
            vertices.extend(ImageVertex::encode(
                pattern,
                premultiplied,
                space,
                points,
                mesh_to_root,
                self.view,
            ));
        }
        if vertices.is_empty() {
            return;
        }
        let buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("theoria WebGPU image triangles"),
                contents: bytes_of_slice(&vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
        let sampler = image_sampler(self.device, pattern, icc_pipeline.is_some());
        let binding = match icc_pipeline {
            Some(icc) => self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("theoria WebGPU ICC image"),
                layout: &icc.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::Sampler(&sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&image.view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: icc.tables.as_entire_binding(),
                    },
                ],
            }),
            None => sampled_binding(
                self.device,
                &self.pipelines.texture_layout,
                &sampler,
                &image.view,
                "theoria WebGPU sampled image",
            ),
        };
        let region = self
            .target_region
            .expect("Canvas drawing is skipped for an empty target region");
        let attachment = load_attachment(&self.scratch_images[accumulator].view);
        let mut pass = self.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("theoria WebGPU sampled image"),
            color_attachments: &[Some(attachment)],
            ..Default::default()
        });
        set_canvas_region(&mut pass, region);
        pass.set_pipeline(icc_pipeline.map_or(&self.pipelines.image, |icc| &icc.pipeline));
        pass.set_bind_group(0, &binding, &[]);
        pass.set_vertex_buffer(0, buffer.slice(..));
        pass.draw(0..vertices.len() as u32, 0..1);
    }

    fn draw_composite(&mut self, accumulator: usize, source: usize, composite: Composite) {
        let binding = self.scratch_binding(source, "theoria WebGPU composite source");
        let attachment = load_attachment(&self.scratch_images[accumulator].view);
        let mut pass = self.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("theoria WebGPU composite"),
            color_attachments: &[Some(attachment)],
            ..Default::default()
        });
        set_full_region(&mut pass, self.size);
        pass.set_pipeline(&self.pipelines.composites[&composite]);
        pass.set_bind_group(0, &binding, &[]);
        pass.draw(0..3, 0..1);
    }

    fn apply_mask(&mut self, content: usize, mask: usize) {
        let binding = self.scratch_binding(mask, "theoria WebGPU coverage mask");
        let attachment = load_attachment(&self.scratch_images[content].view);
        let mut pass = self.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("theoria WebGPU coverage mask"),
            color_attachments: &[Some(attachment)],
            ..Default::default()
        });
        set_full_region(&mut pass, self.size);
        pass.set_pipeline(&self.pipelines.mask);
        pass.set_bind_group(0, &binding, &[]);
        pass.draw(0..3, 0..1);
    }

    fn scratch_binding(&self, image: usize, label: &'static str) -> wgpu::BindGroup {
        sampled_binding(
            self.device,
            &self.pipelines.texture_layout,
            self.composite_sampler,
            &self.scratch_images[image].view,
            label,
        )
    }

    fn clear(&mut self, image: usize) {
        let attachment = clear_attachment(&self.scratch_images[image].view);
        self.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("theoria WebGPU clear Canvas accumulator"),
            color_attachments: &[Some(attachment)],
            ..Default::default()
        });
    }

    fn path_tolerance(&self, path: &Path, path_to_root: Projective2) -> f32 {
        let Some(region) = self.target_region else {
            return DEFAULT_TESSELLATION_TOLERANCE;
        };
        let path_to_view = path_to_root.then(self.view.canvas_to_view);
        screen_space_tolerance(path, path_to_view, region.width, region.height)
    }
}

#[derive(Clone, Copy)]
struct TargetRegion {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    scissor_x: u32,
    scissor_y: u32,
    scissor_width: u32,
    scissor_height: u32,
}

fn set_canvas_region(pass: &mut wgpu::RenderPass<'_>, region: TargetRegion) {
    pass.set_viewport(region.x, region.y, region.width, region.height, 0.0, 1.0);
    pass.set_scissor_rect(
        region.scissor_x,
        region.scissor_y,
        region.scissor_width,
        region.scissor_height,
    );
}

fn target_region(viewport: Viewport, size: SurfaceSize) -> Option<TargetRegion> {
    if !viewport.has_finite_positive_size() {
        return None;
    }
    let left = viewport.x.max(0.0).floor().min(size.width as f32) as u32;
    let top = viewport.y.max(0.0).floor().min(size.height as f32) as u32;
    let right = (viewport.x + viewport.width)
        .max(0.0)
        .ceil()
        .min(size.width as f32) as u32;
    let bottom = (viewport.y + viewport.height)
        .max(0.0)
        .ceil()
        .min(size.height as f32) as u32;
    if right <= left || bottom <= top {
        return None;
    }
    Some(TargetRegion {
        x: viewport.x,
        y: viewport.y,
        width: viewport.width,
        height: viewport.height,
        scissor_x: left,
        scissor_y: top,
        scissor_width: right - left,
        scissor_height: bottom - top,
    })
}

fn sampled_binding(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    view: &wgpu::TextureView,
    label: &'static str,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(label),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(view),
            },
        ],
    })
}

fn image_sampler(
    device: &wgpu::Device,
    pattern: &ImagePattern,
    force_nearest: bool,
) -> wgpu::Sampler {
    let filter = match (force_nearest, pattern.sampling()) {
        (true, _) => wgpu::FilterMode::Nearest,
        (_, ImageSampling::Nearest) => wgpu::FilterMode::Nearest,
        (_, ImageSampling::Linear) => wgpu::FilterMode::Linear,
    };
    device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("theoria WebGPU image sampler"),
        address_mode_u: address_mode(pattern.extend_x()),
        address_mode_v: address_mode(pattern.extend_y()),
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: filter,
        min_filter: filter,
        ..Default::default()
    })
}

fn address_mode(extend: ImageExtend) -> wgpu::AddressMode {
    match extend {
        ImageExtend::Transparent | ImageExtend::Clamp => wgpu::AddressMode::ClampToEdge,
        ImageExtend::Repeat => wgpu::AddressMode::Repeat,
        ImageExtend::Mirror => wgpu::AddressMode::MirrorRepeat,
    }
}

fn clear_attachment(view: &wgpu::TextureView) -> wgpu::RenderPassColorAttachment<'_> {
    wgpu::RenderPassColorAttachment {
        view,
        depth_slice: None,
        resolve_target: None,
        ops: wgpu::Operations {
            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
            store: wgpu::StoreOp::Store,
        },
    }
}

fn load_attachment(view: &wgpu::TextureView) -> wgpu::RenderPassColorAttachment<'_> {
    wgpu::RenderPassColorAttachment {
        view,
        depth_slice: None,
        resolve_target: None,
        ops: wgpu::Operations {
            load: wgpu::LoadOp::Load,
            store: wgpu::StoreOp::Store,
        },
    }
}

fn set_full_region(pass: &mut wgpu::RenderPass<'_>, size: SurfaceSize) {
    pass.set_viewport(0.0, 0.0, size.width as f32, size.height as f32, 0.0, 1.0);
    pass.set_scissor_rect(0, 0, size.width, size.height);
}

#[cfg(test)]
mod tests {
    use std::{
        future::Future,
        sync::{Arc, mpsc},
        task::{Context, Poll, Wake, Waker},
        thread,
    };

    use super::*;
    use crate::{
        canvas::{Composite, Coverage},
        color::{LinearSrgb, Opacity},
        geometry::Point,
        image::Srgba8Image,
    };

    #[test]
    fn target_region_preserves_the_viewport_and_clips_its_scissor() {
        let region = target_region(
            Viewport::new(-0.5, 1.25, 4.0, 3.5),
            SurfaceSize {
                width: 3,
                height: 4,
            },
        )
        .unwrap();

        assert_eq!(
            [region.x, region.y, region.width, region.height],
            [-0.5, 1.25, 4.0, 3.5]
        );
        assert_eq!(
            [
                region.scissor_x,
                region.scissor_y,
                region.scissor_width,
                region.scissor_height,
            ],
            [0, 1, 3, 3]
        );
    }

    #[test]
    fn canvas_renderer_submits_an_offscreen_webgpu_render() {
        let backdrop = Canvas::draw(Color::rgb8(255, 0, 0), Coverage::all());
        let source = Canvas::draw(
            Srgba8Image::new(1, 1, vec![0, 255, 0, 255]).unwrap(),
            Coverage::from_alpha(Color::rgb8(255, 255, 255)),
        );
        let canvas = backdrop.composite(
            source,
            Composition::new(Composite::SourceOver, crate::color::LinearSrgb)
                .with_blend(crate::canvas::BlendMode::Multiply, crate::color::LinearSrgb),
        );
        if let Some(pixel) = render_pixel(&canvas) {
            assert_eq!(pixel, [0, 0, 0, 255]);
        }
    }

    #[test]
    fn selected_working_space_preserves_srgba_output() {
        let profile = IccProfile::from_bytes(include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/crates/theoria-color/src/color/fixtures/DisplayP3-v4.icc"
        )))
        .unwrap();
        let image = crate::canvas::Image::from_icc_u8(
            1,
            1,
            vec![200, 100, 50],
            profile,
            IccRenderingIntent::RelativeColorimetric,
            false,
        )
        .unwrap();
        let view = CanvasView::axis_aligned(Point::new(0.0, 0.0), Point::new(1.0, 1.0)).unwrap();
        let cached = CanvasImage::new(
            Canvas::draw(
                ImagePattern::new(image).with_extend(ImageExtend::Clamp),
                Coverage::all(),
            ),
            view,
            1,
            1,
        )
        .unwrap();
        let backdrop = Canvas::draw(Color::rgb8(32, 96, 180), Coverage::all());
        let source = Canvas::draw(
            ImagePattern::new(cached).with_extend(ImageExtend::Clamp),
            Coverage::all(),
        );
        let canvas = backdrop.composite(
            source,
            Composition::new(Composite::SourceOver, RgbColorSpace::LinearSrgb),
        );
        let linear_srgb = render_pixel_bytes_with_working_space(
            &canvas,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            None,
            RgbColorSpace::LinearSrgb,
        );
        let linear_p3 = render_pixel_bytes_with_working_space(
            &canvas,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            None,
            RgbColorSpace::LinearDisplayP3,
        );
        if let (Some(expected), Some(actual)) = (linear_srgb, linear_p3) {
            for (&actual, &expected) in actual.iter().zip(&expected) {
                assert!(
                    actual.abs_diff(expected) <= 3,
                    "{actual} != {expected}; full: {linear_p3:?} != {linear_srgb:?}"
                );
            }
        }
    }

    #[test]
    fn blend_is_converted_to_composition_space_before_alpha_weighting() {
        let backdrop = Canvas::draw(Color::rgba8(51, 51, 51, 128), Coverage::all());
        let source = Canvas::draw(Color::rgb8(204, 204, 204), Coverage::all());
        let canvas = backdrop.composite(
            source,
            Composition::new(Composite::SourceOver, crate::color::LinearSrgb)
                .with_blend(crate::canvas::BlendMode::Multiply, crate::color::Srgb),
        );
        if let Some(pixel) = render_pixel(&canvas) {
            // Weighting the source and blend results in sRGB instead yields
            // approximately 127; linear-light composition is much brighter.
            assert!(
                (145..=165).contains(&pixel[0]),
                "unexpected pixel: {pixel:?}"
            );
            assert_eq!([pixel[0], pixel[0], pixel[0], 255], pixel);
        }
    }

    #[test]
    fn custom_wgsl_blend_is_used_by_webgpu() {
        let shader = crate::canvas::BlendShader::wgsl("return vec3<f32>(1.0, 0.0, 0.0);").unwrap();
        let backdrop = Canvas::draw(Color::rgb8(0, 255, 0), Coverage::all());
        let source = Canvas::draw(Color::rgb8(0, 0, 255), Coverage::all());
        let canvas = backdrop.composite(
            source,
            Composition::new(Composite::SourceOver, crate::color::LinearSrgb)
                .with_blend(shader, crate::color::LinearSrgb),
        );
        if let Some(pixel) = render_pixel(&canvas) {
            assert_eq!(pixel, [255, 0, 0, 255]);
        }
    }

    #[test]
    fn icc_output_profile_changes_final_webgpu_device_codes() {
        let profile = IccProfile::from_bytes(include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/crates/theoria-color/src/color/fixtures/DisplayP3-v4.icc"
        )))
        .unwrap();
        let canvas = Canvas::draw(Color::rgb8(255, 0, 0), Coverage::all());
        if let Some(pixel) =
            render_pixel_in(&canvas, wgpu::TextureFormat::Rgba8Unorm, Some(&profile))
        {
            let reference = crate::image::IccRasterImage::from_srgba8(
                &Srgba8Image::new(1, 1, vec![255, 0, 0, 255]).unwrap(),
                profile,
                IccRenderingIntent::RelativeColorimetric,
            )
            .unwrap();
            for (actual, expected) in pixel.iter().zip(reference.pixels()) {
                assert!(
                    actual.abs_diff(*expected) <= 3,
                    "{pixel:?} != {:?}",
                    reference.pixels()
                );
            }
            assert_eq!(pixel[3], 255);
            assert!(pixel[0] < 255, "profile conversion was skipped: {pixel:?}");
            assert!(pixel[1] > 0, "profile conversion was skipped: {pixel:?}");
        }
    }

    #[test]
    fn floating_working_image_preserves_out_of_gamut_values() {
        let color = Color::new(LinearSrgb, [1.5, 0.0, 0.0], Opacity::<f32>::ONE).unwrap();
        let canvas = Canvas::draw(color, Coverage::all());
        if let Some(pixel) = render_pixel_bytes_in(&canvas, wgpu::TextureFormat::Rgba16Float, None)
        {
            assert_eq!(u16::from_le_bytes(pixel[..2].try_into().unwrap()), 0x3e00);
        }
    }

    #[test]
    fn icc_raster_keeps_out_of_srgb_gamut_values_when_sampled() {
        let profile = IccProfile::from_bytes(include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/crates/theoria-color/src/color/fixtures/DisplayP3-v4.icc"
        )))
        .unwrap();
        let image = crate::canvas::Image::from_icc_u8(
            1,
            1,
            vec![255, 0, 0],
            profile,
            IccRenderingIntent::RelativeColorimetric,
            false,
        )
        .unwrap();
        let canvas = Canvas::draw(
            ImagePattern::new(image).with_extend(ImageExtend::Clamp),
            Coverage::all(),
        );
        if let Some(pixel) = render_pixel_bytes_in(&canvas, wgpu::TextureFormat::Rgba16Float, None)
        {
            let red =
                half::f16::from_bits(u16::from_le_bytes(pixel[..2].try_into().unwrap())).to_f32();
            assert!(
                red > 1.0,
                "ICC raster red was clipped during sampling: {red}"
            );
        }
    }

    #[test]
    fn cmyk_raster_with_alpha_is_transformed_when_drawn() {
        let profile = IccProfile::from_bytes(include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/crates/theoria-color/src/color/fixtures/CGATS001Compat-v2-micro.icc"
        )))
        .unwrap();
        let samples = [50, 100, 150, 20, 128];
        let expected = crate::canvas_renderer::raster::convert_icc8_to_linear_rgba16(
            1,
            &samples,
            &profile,
            IccRenderingIntent::Perceptual,
            true,
        )
        .unwrap();
        let image = crate::canvas::Image::from_icc_u8(
            1,
            1,
            samples.to_vec(),
            profile,
            IccRenderingIntent::Perceptual,
            true,
        )
        .unwrap();
        let canvas = Canvas::draw(
            ImagePattern::new(image).with_extend(ImageExtend::Clamp),
            Coverage::all(),
        );
        if let Some(pixel) = render_pixel_bytes_in(&canvas, wgpu::TextureFormat::Rgba16Float, None)
        {
            let alpha = half::f16::from_bits(expected.samples()[3]).to_f32();
            for channel in 0..4 {
                let actual = half::f16::from_bits(u16::from_le_bytes(
                    pixel[channel * 2..channel * 2 + 2].try_into().unwrap(),
                ))
                .to_f32();
                let expected = half::f16::from_bits(expected.samples()[channel]).to_f32();
                let expected = if channel == 3 {
                    expected
                } else {
                    expected * alpha
                };
                assert!(
                    (actual - expected).abs() < 0.04,
                    "channel {channel}: {actual} != {expected}"
                );
            }
        }
    }

    fn render_pixel(canvas: &Canvas) -> Option<[u8; 4]> {
        render_pixel_in(canvas, wgpu::TextureFormat::Rgba8UnormSrgb, None)
    }

    fn render_pixel_in(
        canvas: &Canvas,
        format: wgpu::TextureFormat,
        profile: Option<&IccProfile>,
    ) -> Option<[u8; 4]> {
        render_pixel_bytes_in(canvas, format, profile).map(|bytes| bytes[..4].try_into().unwrap())
    }

    fn render_pixel_bytes_in(
        canvas: &Canvas,
        format: wgpu::TextureFormat,
        profile: Option<&IccProfile>,
    ) -> Option<[u8; 8]> {
        render_pixel_bytes_with_working_space(canvas, format, profile, RgbColorSpace::LinearSrgb)
    }

    fn render_pixel_bytes_with_working_space(
        canvas: &Canvas,
        format: wgpu::TextureFormat,
        profile: Option<&IccProfile>,
        working_space: RgbColorSpace,
    ) -> Option<[u8; 8]> {
        block_on(async {
            let instance = wgpu::Instance::default();
            let Ok(adapter) = instance
                .request_adapter(&wgpu::RequestAdapterOptions::default())
                .await
            else {
                return None;
            };
            let Ok((device, queue)) = adapter
                .request_device(&wgpu::DeviceDescriptor::default())
                .await
            else {
                return None;
            };
            let size = SurfaceSize {
                width: 2,
                height: 2,
            };
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("theoria WebGPU Canvas test target"),
                size: wgpu::Extent3d {
                    width: size.width,
                    height: size.height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let target_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            let mut renderer =
                CanvasRenderer::new_with_working_space(device.clone(), queue, working_space);
            let view =
                CanvasView::axis_aligned(Point::new(0.0, 0.0), Point::new(1.0, 1.0)).unwrap();

            let target = RenderTarget::new(&target_view, format, size, Color::rgba8(0, 0, 0, 0));
            let target = match profile {
                Some(profile) => {
                    target.with_icc_output(profile, IccRenderingIntent::RelativeColorimetric)
                }
                None => target,
            };
            renderer
                .render(
                    target,
                    Viewport::new(0.0, 0.0, size.width as f32, size.height as f32),
                    canvas,
                    view,
                )
                .unwrap();

            let bytes_per_row = 256;
            let readback = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("theoria WebGPU Canvas test readback"),
                size: u64::from(bytes_per_row * size.height),
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("theoria WebGPU Canvas test readback"),
            });
            encoder.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyBufferInfo {
                    buffer: &readback,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(bytes_per_row),
                        rows_per_image: Some(size.height),
                    },
                },
                wgpu::Extent3d {
                    width: size.width,
                    height: size.height,
                    depth_or_array_layers: 1,
                },
            );
            renderer.queue.submit([encoder.finish()]);

            let (mapped, completed) = mpsc::channel();
            readback.map_async(wgpu::MapMode::Read, .., move |result| {
                mapped.send(result).unwrap();
            });
            device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
            completed.recv().unwrap().unwrap();
            let bytes = readback.get_mapped_range(..).unwrap();
            Some(bytes[..8].try_into().unwrap())
        })
    }

    struct ThreadWaker(thread::Thread);

    impl Wake for ThreadWaker {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }

        fn wake_by_ref(self: &Arc<Self>) {
            self.0.unpark();
        }
    }

    fn block_on<F: Future>(future: F) -> F::Output {
        let waker = Waker::from(Arc::new(ThreadWaker(thread::current())));
        let mut context = Context::from_waker(&waker);
        let mut future = std::pin::pin!(future);
        loop {
            match future.as_mut().poll(&mut context) {
                Poll::Ready(output) => return output,
                Poll::Pending => thread::park(),
            }
        }
    }
}
