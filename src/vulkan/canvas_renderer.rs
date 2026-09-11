use std::{collections::HashSet, error::Error, sync::Arc};

use ash::vk;

use crate::{
    canvas::{Canvas, CanvasView},
    image::{Image, ImageContent, ImageInstanceKey},
    target::Viewport,
    transform::Projective2,
};

use super::{
    image::{ImageCache, VulkanImage},
    image_layout::transition_image,
    presenter::{RenderDevice, RenderTarget},
    scratch_image::ScratchImage,
};

mod canvas_plan;
mod encoding;
mod mesh_paint;
mod one_shot;
mod pipeline;
mod recording;

use canvas_plan::{collect_images, required_scratch_images};
use one_shot::OneShotCommands;
use pipeline::{Pipelines, create_pipelines};
use recording::CanvasRecorder;

#[cfg(test)]
use canvas_plan::can_record_directly;
#[cfg(test)]
use encoding::ImagePushConstants;
#[cfg(test)]
use mesh_paint::{PathSampler, paint_triangle, point_to_clip, screen_space_tolerance};
#[cfg(test)]
use pipeline::composite_blend_factors;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

/// Translates backend-independent Canvas commands into Vulkan commands.
pub struct CanvasRenderer {
    device: RenderDevice,
    gpu: CanvasGpuResources,
    resolving_images: HashSet<ImageInstanceKey>,
}

struct CanvasGpuResources {
    pipelines: Option<Pipelines>,
    image_cache: Option<ImageCache>,
    scratch_images: Vec<ScratchImage>,
}

impl CanvasRenderer {
    pub fn new(device: RenderDevice) -> Self {
        Self {
            device,
            gpu: CanvasGpuResources {
                pipelines: None,
                image_cache: None,
                scratch_images: Vec::new(),
            },
            resolving_images: HashSet::new(),
        }
    }

    pub fn render(
        &mut self,
        target: RenderTarget<'_>,
        viewport: Viewport,
        canvas: &Canvas,
        view: CanvasView,
    ) -> Result<()> {
        self.materialize_canvas_images(canvas)?;
        let (image_view, format, extent) = target.surface();
        self.record_to_image_view(
            target.command_buffer,
            image_view,
            format,
            extent,
            viewport,
            canvas,
            view,
            target.clear_color().to_premultiplied_array(),
        )
    }

    fn materialize_canvas_image(
        &mut self,
        width: u32,
        height: u32,
        canvas: &Canvas,
        view: CanvasView,
    ) -> Result<Arc<VulkanImage>> {
        let extent = vk::Extent2D { width, height };
        let format = vk::Format::R8G8B8A8_SRGB;
        self.gpu.ensure_image_cache(&self.device)?;
        let backing = self
            .gpu
            .image_cache
            .as_ref()
            .unwrap()
            .create_render_target(format, extent)?;
        let commands = OneShotCommands::begin(self.device.clone())?;
        let command_buffer = commands.buffer();
        unsafe {
            transition_image(
                self.device.raw(),
                command_buffer,
                backing.image,
                vk::ImageLayout::UNDEFINED,
                vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                vk::PipelineStageFlags2::NONE,
                vk::AccessFlags2::NONE,
                vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
            );
        }
        self.record_to_image_view(
            command_buffer,
            backing.view,
            backing.format,
            backing.extent,
            Viewport::new(0.0, 0.0, width as f32, height as f32),
            canvas,
            view,
            [0.0; 4],
        )?;
        unsafe {
            transition_image(
                self.device.raw(),
                command_buffer,
                backing.image,
                vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
                vk::PipelineStageFlags2::FRAGMENT_SHADER,
                vk::AccessFlags2::SHADER_SAMPLED_READ,
            );
        }
        commands.submit_and_wait()?;
        Ok(backing)
    }

    fn materialize_canvas_images(&mut self, canvas: &Canvas) -> Result<()> {
        let mut images = Vec::new();
        collect_images(canvas, &mut images);
        for image in images {
            self.materialize_image(&image)?;
        }
        Ok(())
    }

    fn materialize_image(&mut self, image: &Image) -> Result<()> {
        self.gpu.ensure_image_cache(&self.device)?;
        if self.gpu.image_cache.as_mut().unwrap().contains(image) {
            return Ok(());
        }

        let key = image.instance_key();
        if !self.resolving_images.insert(key.clone()) {
            return Err("cyclic Image producer dependency".into());
        }
        let result = (|| -> Result<()> {
            match image.content() {
                ImageContent::Rgba8(_) => self
                    .gpu
                    .image_cache
                    .as_mut()
                    .unwrap()
                    .ensure_uploaded(image),
                ImageContent::Canvas { canvas, view } => {
                    self.materialize_canvas_images(canvas)?;
                    let backing = self.materialize_canvas_image(
                        image.width(),
                        image.height(),
                        canvas,
                        *view,
                    )?;
                    self.gpu
                        .image_cache
                        .as_mut()
                        .unwrap()
                        .insert_rendered(image, backing)
                }
            }
        })();
        self.resolving_images.remove(&key);
        result
    }

    #[allow(clippy::too_many_arguments)]
    fn record_to_image_view(
        &mut self,
        command_buffer: vk::CommandBuffer,
        image_view: vk::ImageView,
        format: vk::Format,
        extent: vk::Extent2D,
        viewport: Viewport,
        canvas: &Canvas,
        view: CanvasView,
        clear_color: [f32; 4],
    ) -> Result<()> {
        let sample_count = preferred_sample_count(&self.device, format);
        let target_region = target_region(viewport, extent);
        self.gpu.ensure_image_cache(&self.device)?;
        self.gpu
            .ensure_pipelines(&self.device, format, sample_count)?;
        self.gpu.ensure_scratch_images(
            &self.device,
            format,
            extent,
            sample_count,
            required_scratch_images(canvas),
        )?;
        let pipelines = self.gpu.pipelines.as_ref().unwrap();
        let image_cache = self.gpu.image_cache.as_ref().unwrap();
        let device = self.device.raw();
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
                scratch_images: &self.gpu.scratch_images,
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
                        float32: clear_color,
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
                &[self.gpu.scratch_images[canvas_image].descriptor_set()],
                &[],
            );
            device.cmd_draw(command_buffer, 3, 1, 0, 0);
            device.cmd_end_rendering(command_buffer);
        }
        Ok(())
    }
}

impl CanvasGpuResources {
    fn ensure_image_cache(&mut self, device: &RenderDevice) -> Result<()> {
        if self.image_cache.is_none() {
            self.image_cache = Some(ImageCache::new(device.clone())?);
        }
        Ok(())
    }

    fn ensure_pipelines(
        &mut self,
        device: &RenderDevice,
        format: vk::Format,
        sample_count: vk::SampleCountFlags,
    ) -> Result<()> {
        if self.pipelines.as_ref().is_some_and(|pipelines| {
            pipelines.format == format && pipelines.sample_count == sample_count
        }) {
            return Ok(());
        }
        self.destroy_pipelines(device);
        let descriptor_layout = self.image_cache.as_ref().unwrap().descriptor_set_layout();
        self.pipelines = Some(unsafe {
            create_pipelines(device.raw(), format, sample_count, descriptor_layout)?
        });
        Ok(())
    }

    fn ensure_scratch_images(
        &mut self,
        device: &RenderDevice,
        format: vk::Format,
        extent: vk::Extent2D,
        sample_count: vk::SampleCountFlags,
        required: usize,
    ) -> Result<()> {
        if self.scratch_images.first().is_some_and(|image| {
            image.format != format || image.extent != extent || image.sample_count != sample_count
        }) {
            self.scratch_images.clear();
        }
        while self.scratch_images.len() < required {
            self.scratch_images.push(ScratchImage::new(
                device.clone(),
                self.image_cache.as_ref().unwrap(),
                format,
                extent,
                sample_count,
            )?);
        }
        Ok(())
    }

    fn destroy_pipelines(&mut self, device: &RenderDevice) {
        if let Some(pipelines) = self.pipelines.take() {
            unsafe {
                for pipeline in [
                    pipelines.triangle,
                    pipelines.image,
                    pipelines.mask,
                    pipelines.present,
                    pipelines.blend,
                    pipelines.coverage,
                ]
                .into_iter()
                .chain(pipelines.composites.into_values())
                {
                    device.raw().destroy_pipeline(pipeline.handle, None);
                    device.raw().destroy_pipeline_layout(pipeline.layout, None);
                }
            }
        }
    }
}

impl Drop for CanvasRenderer {
    fn drop(&mut self) {
        unsafe {
            let _ = self.device.raw().device_wait_idle();
        }
        self.gpu.scratch_images.clear();
        self.gpu.destroy_pipelines(&self.device);
        self.gpu.image_cache = None;
    }
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

pub(super) fn full_viewport(extent: vk::Extent2D) -> vk::Viewport {
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

#[cfg(test)]
mod tests {
    use super::super::image::GpuAlphaMode;
    use super::*;
    use crate::canvas::{
        ColorField, Composite, ConicGradient, Coverage, GradientExtend, GradientPoint,
        GradientStop, ImagePattern, LinearGradient, MeshGradient, RadialGradient,
    };
    use crate::color::Color;
    use crate::geometry::Point;
    use crate::path::{Path, Subpath};
    use crate::tessellation::DEFAULT_TESSELLATION_TOLERANCE;

    #[test]
    fn image_is_a_raster_boundary_while_canvas_color_field_stays_semantic() {
        let pixels = Image::from_rgba8(1, 1, vec![255; 4]).unwrap();
        let producer = Canvas::draw(pixels.clone(), Coverage::all());
        let view = CanvasView::axis_aligned(Point::new(0.0, 0.0), Point::new(1.0, 1.0)).unwrap();
        let image = Image::from_canvas(1, 1, producer.clone(), view).unwrap();

        let mut image_dependencies = Vec::new();
        collect_images(
            &Canvas::draw(image.clone(), Coverage::all()),
            &mut image_dependencies,
        );
        assert_eq!(image_dependencies.len(), 1);
        assert_eq!(image_dependencies[0].instance_key(), image.instance_key());

        let mut canvas_dependencies = Vec::new();
        collect_images(
            &Canvas::draw(producer, Coverage::all()),
            &mut canvas_dependencies,
        );
        assert_eq!(canvas_dependencies.len(), 1);
        assert_eq!(canvas_dependencies[0].instance_key(), pixels.instance_key());
    }

    #[test]
    fn image_sampling_respects_the_gpu_images_alpha_representation() {
        let straight = Image::from_rgba8(1, 1, vec![255, 128, 64, 128]).unwrap();
        let premultiplied = Image::from_canvas(
            1,
            1,
            Canvas::empty(),
            CanvasView::axis_aligned(Point::new(0.0, 0.0), Point::new(1.0, 1.0)).unwrap(),
        )
        .unwrap();
        let points = [Point::new(0.0, 0.0); 3];
        let view = CanvasView::axis_aligned(Point::new(0.0, 0.0), Point::new(1.0, 1.0)).unwrap();

        let straight_constants = ImagePushConstants::new(
            &ImagePattern::new(straight),
            GpuAlphaMode::Straight,
            points,
            Projective2::IDENTITY,
            view,
        );
        let premultiplied_constants = ImagePushConstants::new(
            &ImagePattern::new(premultiplied),
            GpuAlphaMode::Premultiplied,
            points,
            Projective2::IDENTITY,
            view,
        );

        assert_eq!(straight_constants.vertices[0].uv[3], 0.0);
        assert_eq!(premultiplied_constants.vertices[0].uv[3], 1.0);
    }

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
        let root = Canvas::empty().composite(masked, Composite::SourceOver);

        assert_eq!(required_scratch_images(&root), 3);
    }

    #[test]
    fn non_source_over_composites_create_group_boundaries() {
        let leaf = Canvas::draw(Color::rgb8(255, 255, 255), Coverage::all());
        let grouped = leaf.clone().composite(leaf, Composite::SourceIn);
        let root = Canvas::empty().composite(grouped.clone(), Composite::SourceOver);

        assert!(!can_record_directly(&grouped));
        assert_eq!(required_scratch_images(&grouped), 2);
        assert_eq!(required_scratch_images(&root), 3);
    }

    #[test]
    fn destination_reading_blends_reserve_source_and_backdrop_images() {
        let backdrop = Canvas::draw(Color::rgb8(64, 128, 255), Coverage::all());
        let source = Canvas::draw(Color::rgb8(255, 128, 64), Coverage::all());
        let blended = backdrop.blend_composite(
            source,
            crate::canvas::BlendMode::Multiply,
            Composite::SourceOver,
        );

        assert!(!can_record_directly(&blended));
        assert_eq!(required_scratch_images(&blended), 3);
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
            (Composite::Clear, (zero, zero)),
            (Composite::Copy, (one, zero)),
            (Composite::Destination, (zero, one)),
            (Composite::SourceOver, (one, one_minus_source_alpha)),
            (
                Composite::DestinationOver,
                (one_minus_destination_alpha, one),
            ),
            (Composite::SourceIn, (destination_alpha, zero)),
            (Composite::DestinationIn, (zero, source_alpha)),
            (Composite::SourceOut, (one_minus_destination_alpha, zero)),
            (Composite::DestinationOut, (zero, one_minus_source_alpha)),
            (
                Composite::SourceAtop,
                (destination_alpha, one_minus_source_alpha),
            ),
            (
                Composite::DestinationAtop,
                (one_minus_destination_alpha, source_alpha),
            ),
            (
                Composite::Xor,
                (one_minus_destination_alpha, one_minus_source_alpha),
            ),
        ];

        for (operator, factors) in expected {
            assert_eq!(composite_blend_factors(operator), factors);
        }

        assert_eq!(
            composite_blend_factors(Composite::Plus),
            (vk::BlendFactor::ONE, vk::BlendFactor::ONE)
        );
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
            &ColorField::LinearGradient(gradient),
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
            &ColorField::LinearGradient(gradient),
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
        let triangles = paint_triangle(positions, &ColorField::MeshGradient(gradient));

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
            &ColorField::MeshGradient(gradient),
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
            &ColorField::RadialGradient(gradient),
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
    fn two_circle_radial_gradient_is_transparent_where_no_circle_reaches() {
        let gradient = RadialGradient::between_circles(
            Point::new(0.0, 0.0),
            1.0,
            Point::new(10.0, 0.0),
            1.0,
            [
                GradientStop::new(0.0, Color::rgb8(255, 0, 0)).unwrap(),
                GradientStop::new(1.0, Color::rgb8(0, 0, 255)).unwrap(),
            ],
        )
        .unwrap();
        let triangles = paint_triangle(
            [
                Point::new(0.0, 100.0),
                Point::new(1.0, 100.0),
                Point::new(0.0, 101.0),
            ],
            &ColorField::RadialGradient(gradient),
        );

        assert!(
            triangles
                .iter()
                .flat_map(|triangle| triangle.colors)
                .all(|color| color == [0.0; 4])
        );
    }

    #[test]
    fn coincident_radial_circles_paint_nothing() {
        let gradient = RadialGradient::between_circles(
            Point::new(2.0, 3.0),
            4.0,
            Point::new(2.0, 3.0),
            4.0,
            [
                GradientStop::new(0.0, Color::rgb8(255, 0, 0)).unwrap(),
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
            &ColorField::RadialGradient(gradient),
        );

        assert!(
            triangles
                .iter()
                .flat_map(|triangle| triangle.colors)
                .all(|color| color == [0.0; 4])
        );
    }

    #[test]
    fn signed_radial_gradient_uses_only_nonnegative_radius_solutions() {
        let red = Color::rgb8(255, 0, 0);
        let blue = Color::rgb8(0, 0, 255);
        let gradient = RadialGradient::between_circles(
            Point::new(0.0, 0.0),
            -1.0,
            Point::new(0.0, 0.0),
            1.0,
            [
                GradientStop::new(0.0, red).unwrap(),
                GradientStop::new(1.0, blue).unwrap(),
            ],
        )
        .unwrap();
        let triangles = paint_triangle(
            [
                Point::new(1.0, 0.0),
                Point::new(0.0, 1.0),
                Point::new(-1.0, 0.0),
            ],
            &ColorField::RadialGradient(gradient),
        );

        for point in [
            Point::new(1.0, 0.0),
            Point::new(0.0, 1.0),
            Point::new(-1.0, 0.0),
        ] {
            assert!(triangles.iter().any(|triangle| {
                triangle
                    .points
                    .iter()
                    .zip(triangle.colors)
                    .any(|(candidate, color)| {
                        *candidate == point && color == blue.to_premultiplied_array()
                    })
            }));
        }
    }

    #[test]
    fn negative_radial_endpoints_can_reach_a_positive_extended_cone() {
        let red = Color::rgb8(255, 0, 0);
        let gradient = RadialGradient::between_circles(
            Point::new(0.0, 0.0),
            -1.0,
            Point::new(0.0, 0.0),
            -2.0,
            [
                GradientStop::new(0.0, red).unwrap(),
                GradientStop::new(1.0, Color::rgb8(0, 0, 255)).unwrap(),
            ],
        )
        .unwrap();
        let triangles = paint_triangle(
            [
                Point::new(1.0, 0.0),
                Point::new(0.0, 1.0),
                Point::new(-1.0, 0.0),
            ],
            &ColorField::RadialGradient(gradient),
        );

        assert!(triangles.iter().any(|triangle| {
            triangle
                .points
                .iter()
                .zip(triangle.colors)
                .any(|(point, color)| {
                    *point == Point::new(1.0, 0.0) && color == red.to_premultiplied_array()
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
            &ColorField::ConicGradient(gradient),
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

    #[test]
    fn partial_conic_gradient_pads_on_the_absolute_turn() {
        let red = Color::rgb8(255, 0, 0);
        let blue = Color::rgb8(0, 0, 255);
        let gradient = ConicGradient::sweeping(
            Point::new(0.0, 0.0),
            std::f32::consts::FRAC_PI_2,
            std::f32::consts::PI,
            [
                GradientStop::new(0.0, red).unwrap(),
                GradientStop::new(1.0, blue).unwrap(),
            ],
        )
        .unwrap();
        let triangles = paint_triangle(
            [
                Point::new(10.0, 0.0),
                Point::new(0.0, 10.0),
                Point::new(-10.0, 0.0),
            ],
            &ColorField::ConicGradient(gradient),
        );

        let color_at = |point| {
            triangles.iter().find_map(|triangle| {
                triangle
                    .points
                    .iter()
                    .position(|candidate| *candidate == point)
                    .map(|index| triangle.colors[index])
            })
        };
        assert_eq!(
            color_at(Point::new(10.0, 0.0)),
            Some(red.to_premultiplied_array())
        );
        assert_eq!(
            color_at(Point::new(-10.0, 0.0)),
            Some(blue.to_premultiplied_array())
        );
    }

    #[test]
    fn coincident_conic_angles_form_a_pad_step() {
        let red = Color::rgb8(255, 0, 0);
        let blue = Color::rgb8(0, 0, 255);
        let gradient = ConicGradient::sweeping(
            Point::new(0.0, 0.0),
            std::f32::consts::FRAC_PI_2,
            std::f32::consts::FRAC_PI_2,
            [
                GradientStop::new(0.0, red).unwrap(),
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
            &ColorField::ConicGradient(gradient),
        );

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
                    *point == Point::new(-10.0, 0.0) && color == blue.to_premultiplied_array()
                })
        }));
    }

    #[test]
    fn coincident_repeating_conic_angles_paint_nothing() {
        let gradient = ConicGradient::sweeping(
            Point::new(0.0, 0.0),
            0.0,
            0.0,
            [
                GradientStop::new(0.0, Color::rgb8(255, 0, 0)).unwrap(),
                GradientStop::new(1.0, Color::rgb8(0, 0, 255)).unwrap(),
            ],
        )
        .unwrap()
        .with_extend(GradientExtend::Repeat);
        let triangles = paint_triangle(
            [
                Point::new(0.0, 0.0),
                Point::new(10.0, 0.0),
                Point::new(0.0, 10.0),
            ],
            &ColorField::ConicGradient(gradient),
        );

        assert!(
            triangles
                .iter()
                .flat_map(|triangle| triangle.colors)
                .all(|color| color == [0.0; 4])
        );
    }
}
