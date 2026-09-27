use std::{error::Error, sync::Arc};

use ash::vk;

use crate::{
    canvas::{
        Canvas, CanvasView,
        canvas_image::CanvasImage,
        image::{Image, ImageKind},
    },
    color::{ColorProfile, LinearSrgb, Srgb},
    surface::{OutputAlpha, OutputImage, Viewport},
};

use super::{
    color::PremultipliedWorkingRgba,
    image::VulkanImage,
    image_layout::transition_image,
    presenter::{RenderDevice, RenderTarget},
};

mod encoding;
mod pipeline;
mod recording;
mod resources;

use crate::canvas_renderer::canvas_plan::{
    collect_compositions_in, collect_images, prepare_colors_in, required_scratch_images_in,
};
use crate::canvas_renderer::color_space::PreparedColors;
use crate::vulkan::one_shot::OneShotCommands;
use recording::CanvasRecorder;
use resources::CanvasGpuResources;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

/// Translates backend-independent Canvas commands into Vulkan commands.
pub struct CanvasRenderer {
    device: RenderDevice,
    working_profile: ColorProfile,
    gpu: CanvasGpuResources,
}

impl CanvasRenderer {
    pub fn new(device: RenderDevice) -> Self {
        Self::new_with_working_space(device, LinearSrgb)
    }

    /// Selects the RGB coordinates used for intermediate Canvas evaluation.
    /// The choice is fixed for this renderer so cached Canvas images keep one meaning.
    pub fn new_with_working_space(
        device: RenderDevice,
        working_profile: impl Into<ColorProfile>,
    ) -> Self {
        Self {
            device,
            working_profile: working_profile.into(),
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
        let prepared = prepare_colors_in(canvas, &self.working_profile)?;
        let clear_color = target.clear_color();
        clear_color.try_straight_rgba_in(&self.working_profile)?;
        let output = target.output().clone();
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
            PremultipliedWorkingRgba::from_components(
                crate::canvas_renderer::color_space::lower_straight_color_in(
                    &clear_color,
                    &self.working_profile,
                ),
            ),
            Some(&output),
            false,
            &prepared,
        )
    }

    fn materialize_canvas_image_backing(
        &mut self,
        width: u32,
        height: u32,
        canvas: &Canvas,
        view: CanvasView,
    ) -> Result<Arc<VulkanImage>> {
        let prepared = prepare_colors_in(canvas, &self.working_profile)?;
        let extent = vk::Extent2D { width, height };
        // A CanvasImage is an intermediate result, not an output image. Keep
        // it in the same working format as the renderer's scratch images.
        let format = self.working_format();
        self.gpu
            .ensure_image_cache(&self.device, &self.working_profile)?;
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
            PremultipliedWorkingRgba::TRANSPARENT,
            None,
            true,
            &prepared,
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
                self.gpu
                    .ensure_image_cache(&self.device, &self.working_profile)?;
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
        self.gpu
            .ensure_image_cache(&self.device, &self.working_profile)?;
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
        clear_color: PremultipliedWorkingRgba,
        output: Option<&OutputImage>,
        preserve_working: bool,
        prepared: &PreparedColors,
    ) -> Result<()> {
        let hardware_srgb = matches!(
            format,
            vk::Format::R8G8B8A8_SRGB | vk::Format::B8G8R8A8_SRGB
        );
        if hardware_srgb && output.is_some_and(|output| output.profile != Srgb.into()) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "an sRGB attachment requires an sRGB output profile",
            )
            .into());
        }
        if hardware_srgb && output.is_some_and(|output| output.alpha == OutputAlpha::Premultiplied)
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "an sRGB attachment cannot encode premultiplied sRGB components",
            )
            .into());
        }
        let shader_profile = output.map(|output| {
            if hardware_srgb {
                ColorProfile::from(LinearSrgb)
            } else {
                output.profile.clone()
            }
        });
        let working_format = self.working_format();
        let sample_count = preferred_sample_count(&self.device, working_format);
        let target_region = target_region(viewport, extent);
        self.gpu
            .ensure_image_cache(&self.device, &self.working_profile)?;
        self.gpu.ensure_pipelines(
            &self.device,
            working_format,
            format,
            sample_count,
            &self.working_profile,
        )?;
        let icc_layout = self
            .gpu
            .image_cache
            .as_ref()
            .unwrap()
            .icc_descriptor_set_layout();
        let icc_shaders = self.gpu.image_cache.as_mut().unwrap().active_icc_shaders();
        let active_icc = icc_shaders.iter().map(|(key, _)| key.clone()).collect();
        unsafe {
            let pipelines = self.gpu.pipelines.as_mut().unwrap();
            for (key, source) in icc_shaders {
                pipelines.ensure_icc_image(self.device.raw(), icc_layout, key, &source)?;
            }
            pipelines.retain_icc_images(self.device.raw(), &active_icc);
        }
        if let (Some(output), Some(profile)) = (output, &shader_profile) {
            let descriptor_layout = self
                .gpu
                .image_cache
                .as_ref()
                .unwrap()
                .descriptor_set_layout();
            unsafe {
                self.gpu.pipelines.as_mut().unwrap().ensure_output(
                    self.device.raw(),
                    descriptor_layout,
                    profile,
                    output.intent,
                )?;
            }
        }
        let mut compositions = Vec::new();
        collect_compositions_in(canvas, &mut compositions, &self.working_profile);
        self.gpu.ensure_compositions(&self.device, &compositions)?;
        self.gpu.ensure_scratch_images(
            &self.device,
            working_format,
            extent,
            sample_count,
            required_scratch_images_in(canvas, &self.working_profile),
        )?;
        let pipelines = self.gpu.pipelines.as_ref().unwrap();
        let present_pipeline = match (output, &shader_profile) {
            (Some(output), Some(profile)) => pipelines.output(profile, output.intent),
            (None, _) if preserve_working => {
                pipelines.composite(crate::canvas::Composite::SourceOver)
            }
            _ => unreachable!("an external target specifies an output image"),
        };
        let clear_color = match output {
            Some(_) => PremultipliedWorkingRgba::from_components(
                crate::canvas_renderer::color_space::convert_premultiplied_space(
                    clear_color.components(),
                    &self.working_profile,
                    &ColorProfile::from(LinearSrgb),
                )?,
            ),
            None => clear_color,
        };
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
            prepared,
        );
        recorder.record(
            canvas,
            image_view,
            clear_color,
            present_pipeline,
            output.map(|output| output.alpha),
        );
        Ok(())
    }

    fn working_format(&self) -> vk::Format {
        if self
            .device
            .supports_canvas_working_format(vk::Format::R16G16B16A16_SFLOAT)
        {
            vk::Format::R16G16B16A16_SFLOAT
        } else {
            vk::Format::R8G8B8A8_UNORM
        }
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
