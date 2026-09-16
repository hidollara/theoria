//! Offscreen Canvas evaluation into CPU-readable sRGBA8 images.

use std::error::Error;

use ash::vk;

use crate::{
    image::Srgba8Image,
    surface::{SurfaceContent, SurfaceSize, Viewport},
    vulkan::{
        CanvasRenderer,
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
/// device and returns CPU-readable straight-alpha RGBA8 pixels. It is suitable
/// for image generation, visual tests, and as the frame producer beneath a
/// recorder. Rendering is synchronous: the returned image is complete when
/// [`Self::render`] returns.
pub struct ImageSurface {
    size: SurfaceSize,
    renderer: CanvasRenderer,
}

impl ImageSurface {
    /// Creates an offscreen image Surface with a fixed non-zero extent.
    pub fn new(size: SurfaceSize) -> Result<Self> {
        validate_size(size)?;
        let device = RenderDevice::independent()?;
        Ok(Self {
            size,
            renderer: CanvasRenderer::new(device),
        })
    }

    /// Returns the output image extent.
    pub const fn size(&self) -> SurfaceSize {
        self.size
    }

    /// Evaluates the latest content into one CPU-readable sRGBA8 image.
    pub fn render(&mut self, content: &SurfaceContent) -> Result<Srgba8Image> {
        let size = self.size;
        let byte_len = pixel_byte_len(size)?;
        let extent = vk::Extent2D {
            width: size.width,
            height: size.height,
        };
        let device = self.renderer.device();
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
        let commands = OneShotCommands::begin(device.clone())?;
        let command_buffer = commands.buffer();

        unsafe {
            transition_image(
                device.raw(),
                command_buffer,
                target.image,
                vk::ImageLayout::UNDEFINED,
                vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                vk::PipelineStageFlags2::NONE,
                vk::AccessFlags2::NONE,
                vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
            );
        }
        self.renderer.render(
            RenderTarget::new(
                command_buffer,
                target.view,
                target.format,
                target.extent,
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
                target.image,
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
                target.image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                readback.raw(),
                &[copy],
            );
        }
        commands.submit_and_wait()?;

        let mut pixels = readback.read()?;
        linear_premultiplied_to_srgba_straight(&mut pixels);
        Ok(Srgba8Image::new(size.width, size.height, pixels)?)
    }
}

fn validate_size(size: SurfaceSize) -> Result<()> {
    let _ = pixel_byte_len(size)?;
    Ok(())
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

fn linear_premultiplied_to_srgba_straight(pixels: &mut [u8]) {
    for pixel in pixels.chunks_exact_mut(4) {
        let alpha = f32::from(pixel[3]) / 255.0;
        if alpha == 0.0 {
            pixel[..3].fill(0);
            continue;
        }
        for component in &mut pixel[..3] {
            let linear = (f32::from(*component) / 255.0 / alpha).clamp(0.0, 1.0);
            let srgb = if linear <= 0.003_130_8 {
                linear * 12.92
            } else {
                1.055 * linear.powf(1.0 / 2.4) - 0.055
            };
            *component = (srgb * 255.0).round() as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{linear_premultiplied_to_srgba_straight, pixel_byte_len};
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

    #[test]
    fn readback_becomes_straight_alpha_srgba() {
        let mut pixels = [128, 0, 0, 128, 0, 0, 0, 0];

        linear_premultiplied_to_srgba_straight(&mut pixels);

        assert_eq!(pixels, [255, 0, 0, 128, 0, 0, 0, 0]);
    }
}
