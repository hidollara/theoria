//! Offscreen Canvas evaluation into CPU-readable sRGBA8 images.

use std::error::Error;

use ash::vk;

use crate::{
    color::{IccProfile, IccRenderingIntent},
    image::{IccRasterImage, Srgba8Image},
    surface::{SurfaceContent, SurfaceSize, Viewport},
    vulkan::{
        CanvasRenderer,
        color::encode_linear_premultiplied_rgba8_as_straight_srgba8,
        host_buffer::HostBuffer,
        image::VulkanImage,
        image_layout::transition_image,
        one_shot::OneShotCommands,
        presenter::{RenderDevice, RenderTarget},
    },
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

/// A finite offscreen Vulkan Surface that evaluates [`SurfaceContent`] into an sRGBA8 image.
///
/// Unlike a presentation Surface, this Surface owns an independent rendering
/// device and returns CPU-readable straight-alpha sRGBA8 pixels. It is suitable
/// for image generation, visual tests, and as the frame producer beneath a
/// recorder. Rendering is synchronous: the returned image is complete when
/// [`Self::render`] returns.
pub struct ImageSurface {
    size: SurfaceSize,
    renderer: CanvasRenderer,
    target: VulkanImage,
    readback: HostBuffer,
    target_initialized: bool,
}

impl ImageSurface {
    /// Creates an offscreen image Surface with a fixed non-zero extent.
    pub fn new(size: SurfaceSize) -> Result<Self> {
        let byte_len = pixel_byte_len(size)?;
        let device = RenderDevice::independent()?;
        let extent = vk::Extent2D {
            width: size.width,
            height: size.height,
        };
        let target = VulkanImage::new(
            device.clone(),
            vk::Format::R8G8B8A8_UNORM,
            extent,
            vk::SampleCountFlags::TYPE_1,
            vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_SRC,
        )?;
        let readback = HostBuffer::new(
            device.clone(),
            byte_len as vk::DeviceSize,
            vk::BufferUsageFlags::TRANSFER_DST,
        )?;
        Ok(Self {
            size,
            renderer: CanvasRenderer::new(device),
            target,
            readback,
            target_initialized: false,
        })
    }

    /// Returns the output image extent.
    pub const fn size(&self) -> SurfaceSize {
        self.size
    }

    /// Evaluates the latest content into one CPU-readable sRGBA8 image.
    pub fn render(&mut self, content: &SurfaceContent) -> Result<Srgba8Image> {
        let size = self.size;
        let device = self.renderer.device();
        let commands = OneShotCommands::begin(device.clone())?;
        let command_buffer = commands.buffer();

        let (old_layout, source_stage, source_access) = if self.target_initialized {
            (
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                vk::PipelineStageFlags2::COPY,
                vk::AccessFlags2::TRANSFER_READ,
            )
        } else {
            (
                vk::ImageLayout::UNDEFINED,
                vk::PipelineStageFlags2::NONE,
                vk::AccessFlags2::NONE,
            )
        };
        unsafe {
            transition_image(
                device.raw(),
                command_buffer,
                self.target.image,
                old_layout,
                vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                source_stage,
                source_access,
                vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
            );
        }
        self.renderer.render(
            RenderTarget::new(
                command_buffer,
                self.target.view,
                self.target.format,
                self.target.extent,
                content.clear_color(),
            ),
            Viewport::new(0.0, 0.0, size.width as f32, size.height as f32),
            content.canvas(),
            content.view(),
        )?;
        unsafe {
            transition_image(
                device.raw(),
                command_buffer,
                self.target.image,
                vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
                vk::PipelineStageFlags2::COPY,
                vk::AccessFlags2::TRANSFER_READ,
            );
            let copy = vk::BufferImageCopy::default()
                .image_subresource(
                    vk::ImageSubresourceLayers::default()
                        .aspect_mask(vk::ImageAspectFlags::COLOR)
                        .layer_count(1),
                )
                .image_extent(vk::Extent3D {
                    width: size.width,
                    height: size.height,
                    depth: 1,
                });
            device.raw().cmd_copy_image_to_buffer(
                command_buffer,
                self.target.image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                self.readback.raw(),
                &[copy],
            );
        }
        commands.submit_and_wait()?;
        self.target_initialized = true;

        let mut pixels = self.readback.read()?;
        encode_linear_premultiplied_rgba8_as_straight_srgba8(&mut pixels);
        Ok(Srgba8Image::new(size.width, size.height, pixels)?)
    }

    /// Renders to a CPU-readable image encoded in the selected ICC profile.
    /// The existing sRGBA8 readback is used as an intermediate until a GPU
    /// output-transfer path is available; source Canvas colors stay unchanged.
    pub fn render_icc(
        &mut self,
        content: &SurfaceContent,
        profile: IccProfile,
        intent: IccRenderingIntent,
    ) -> Result<IccRasterImage> {
        let image = self.render(content)?;
        IccRasterImage::from_srgba8(&image, profile, intent)
    }
}

fn pixel_byte_len(size: SurfaceSize) -> Result<usize> {
    if size.width == 0 || size.height == 0 {
        return Err(format!(
            "image Surface dimensions must be non-zero, got {}x{}",
            size.width, size.height
        )
        .into());
    }
    usize::try_from(size.width)
        .ok()
        .and_then(|width| {
            usize::try_from(size.height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| {
            format!(
                "image Surface dimensions are too large: {}x{}",
                size.width, size.height
            )
            .into()
        })
}

#[cfg(test)]
mod tests {
    use super::pixel_byte_len;
    use crate::surface::SurfaceSize;

    #[test]
    fn image_surface_extent_must_be_non_zero_and_fit_memory() {
        assert!(
            pixel_byte_len(SurfaceSize {
                width: 1,
                height: 1
            })
            .is_ok()
        );
        assert!(
            pixel_byte_len(SurfaceSize {
                width: 0,
                height: 1
            })
            .is_err()
        );
        assert!(
            pixel_byte_len(SurfaceSize {
                width: 1,
                height: 0
            })
            .is_err()
        );
    }
}
