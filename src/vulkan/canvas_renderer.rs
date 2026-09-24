use std::{error::Error, sync::Arc};

use ash::vk;

use crate::{
    canvas::{
        Canvas, CanvasView,
        canvas_image::CanvasImage,
        image::{Image, ImageKind},
    },
    surface::Viewport,
};

use super::{
    color::PremultipliedLinearSrgba,
    image::VulkanImage,
    image_layout::transition_image,
    presenter::{RenderDevice, RenderTarget},
};

mod encoding;
mod pipeline;
mod recording;
mod resources;

use crate::canvas_renderer::canvas_plan::{
    collect_blend_shaders, collect_images, prepare_colors, required_scratch_images,
};
use crate::vulkan::one_shot::OneShotCommands;
use recording::CanvasRecorder;
use resources::CanvasGpuResources;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

/// Translates backend-independent Canvas commands into Vulkan commands.
pub struct CanvasRenderer {
    device: RenderDevice,
    gpu: CanvasGpuResources,
}

impl CanvasRenderer {
    pub fn new(device: RenderDevice) -> Self {
        Self {
            device,
            gpu: CanvasGpuResources::new(),
        }
    }

    pub(crate) fn device(&self) -> RenderDevice {
        self.device.clone()
    }

    pub fn render(
        &mut self,
        target: RenderTarget<'_>,
        viewport: Viewport,
        canvas: &Canvas,
        view: CanvasView,
    ) -> Result<()> {
        prepare_colors(canvas)?;
        let clear_color = target.clear_color();
        clear_color.prepare_linear_srgb()?;
        self.materialize_canvas_resources(canvas)?;
        let (image_view, format, extent) = target.surface();
        self.record_to_image_view(
            target.command_buffer,
            image_view,
            format,
            extent,
            viewport,
            canvas,
            view,
            PremultipliedLinearSrgba::from_color(clear_color),
        )
    }

    fn materialize_canvas_image_backing(
        &mut self,
        width: u32,
        height: u32,
        canvas: &Canvas,
        view: CanvasView,
    ) -> Result<Arc<VulkanImage>> {
        prepare_colors(canvas)?;
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
            PremultipliedLinearSrgba::TRANSPARENT,
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

    fn materialize_canvas_resources(&mut self, canvas: &Canvas) -> Result<()> {
        let mut images = Vec::new();
        collect_images(canvas, &mut images);
        for image in images {
            self.materialize_image(&image)?;
        }
        Ok(())
    }

    fn materialize_image(&mut self, image: &Image) -> Result<()> {
        match image.kind() {
            ImageKind::Raster(image) => {
                self.gpu.ensure_image_cache(&self.device)?;
                self.gpu
                    .image_cache
                    .as_mut()
                    .unwrap()
                    .ensure_uploaded(image.as_ref())
            }
            ImageKind::Canvas(image) => self.materialize_canvas_image(image),
        }
    }

    fn materialize_canvas_image(&mut self, image: &CanvasImage) -> Result<()> {
        self.gpu.ensure_image_cache(&self.device)?;
        if self
            .gpu
            .image_cache
            .as_mut()
            .unwrap()
            .contains_canvas(image)
        {
            return Ok(());
        }

        self.materialize_canvas_resources(image.content())?;
        let backing = self.materialize_canvas_image_backing(
            image.width(),
            image.height(),
            image.content(),
            image.view(),
        )?;
        self.gpu
            .image_cache
            .as_mut()
            .unwrap()
            .insert_canvas(image, backing)
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
        clear_color: PremultipliedLinearSrgba,
    ) -> Result<()> {
        let sample_count = preferred_sample_count(&self.device, format);
        let target_region = target_region(viewport, extent);
        self.gpu.ensure_image_cache(&self.device)?;
        self.gpu
            .ensure_pipelines(&self.device, format, sample_count)?;
        let mut blend_shaders = Vec::new();
        collect_blend_shaders(canvas, &mut blend_shaders);
        self.gpu
            .ensure_blend_shaders(&self.device, &blend_shaders)?;
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

        let mut recorder = CanvasRecorder::new(
            device,
            command_buffer,
            pipelines,
            image_cache,
            &self.gpu.scratch_images,
            attachment_area,
            target_region,
            view,
        );
        recorder.record(canvas, image_view, clear_color);
        Ok(())
    }
}

impl Drop for CanvasRenderer {
    fn drop(&mut self) {
        unsafe {
            let _ = self.device.raw().device_wait_idle();
        }
        self.gpu.destroy(&self.device);
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
mod tests;
