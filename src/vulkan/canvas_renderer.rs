use std::{collections::HashSet, error::Error, sync::Arc};

use ash::vk;

use crate::{
    canvas::{Canvas, CanvasView},
    image::{Image, ImageContent, ImageInstanceKey},
    target::Viewport,
    transform::Projective2,
};

use super::{
    image::VulkanImage,
    image_layout::transition_image,
    presenter::{RenderDevice, RenderTarget},
};

mod canvas_plan;
mod encoding;
mod mesh_paint;
mod one_shot;
mod pipeline;
mod recording;
mod resources;

use canvas_plan::{collect_images, required_scratch_images};
use one_shot::OneShotCommands;
use recording::CanvasRecorder;
use resources::CanvasGpuResources;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

/// Translates backend-independent Canvas commands into Vulkan commands.
pub struct CanvasRenderer {
    device: RenderDevice,
    gpu: CanvasGpuResources,
    resolving_images: HashSet<ImageInstanceKey>,
}

impl CanvasRenderer {
    pub fn new(device: RenderDevice) -> Self {
        Self {
            device,
            gpu: CanvasGpuResources::new(),
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
mod tests;
