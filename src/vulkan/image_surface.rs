//! Offscreen Canvas evaluation into CPU-readable sRGBA8 images.

use std::error::Error;

use ash::vk;

use crate::{
    color::{ColorProfile, IccProfile, IccRenderingIntent, LinearSrgb},
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
        Self::new_with_working_space(size, LinearSrgb)
    }

    /// Creates an offscreen Surface using the specified RGB working space.
    /// Readback is still returned as sRGBA8 unless an ICC output is requested.
    pub fn new_with_working_space(
        size: SurfaceSize,
        working_profile: impl Into<ColorProfile>,
    ) -> Result<Self> {
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
            renderer: CanvasRenderer::new_with_working_space(device, working_profile),
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
        let mut pixels = self.render_raw(content, None)?;
        encode_linear_premultiplied_rgba8_as_straight_srgba8(&mut pixels);
        Ok(Srgba8Image::new(size.width, size.height, pixels)?)
    }

    /// Renders directly into ICC device RGB coordinates on the GPU and returns
    /// straight-alpha samples suitable for embedding with the same profile.
    pub fn render_icc(
        &mut self,
        content: &SurfaceContent,
        profile: IccProfile,
        intent: IccRenderingIntent,
    ) -> Result<IccRasterImage> {
        let size = self.size;
        let mut pixels = self.render_raw(content, Some((&profile, intent)))?;
        unpremultiply_rgba8(&mut pixels);
        Ok(IccRasterImage::new(
            size.width,
            size.height,
            pixels,
            profile,
            intent,
            true,
        )?)
    }

    fn render_raw(
        &mut self,
        content: &SurfaceContent,
        output_profile: Option<(&IccProfile, IccRenderingIntent)>,
    ) -> Result<Vec<u8>> {
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
        let target = RenderTarget::new(
            command_buffer,
            self.target.view,
            self.target.format,
            self.target.extent,
            content.clear_color(),
        );
        let target = match output_profile {
            Some((profile, intent)) => target.with_icc_output(profile, intent),
            None => target,
        };
        self.renderer.render(
            target,
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

        self.readback.read()
    }
}

fn unpremultiply_rgba8(pixels: &mut [u8]) {
    for pixel in pixels.chunks_exact_mut(4) {
        let alpha = pixel[3];
        if alpha == 0 {
            pixel[..3].fill(0);
            continue;
        }
        for component in &mut pixel[..3] {
            *component = (f32::from(*component) * 255.0 / f32::from(alpha))
                .round()
                .clamp(0.0, 255.0) as u8;
        }
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
    use super::{ImageSurface, pixel_byte_len};
    use crate::{
        canvas::{Canvas, CanvasView, Composite, Composition, Coverage, ImageExtend, ImagePattern},
        color::{
            Color, IccProfile, IccRenderingIntent, LinearDisplayP3, LinearSrgb, Opacity, Srgb,
        },
        geometry::Point,
        surface::{SurfaceContent, SurfaceSize},
    };

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
    fn selected_working_space_preserves_srgba_readback() {
        let size = SurfaceSize {
            width: 2,
            height: 2,
        };
        let Ok(mut linear_srgb) = ImageSurface::new(size) else {
            return; // Vulkan may be unavailable on the test host.
        };
        let mut linear_p3 = ImageSurface::new_with_working_space(size, LinearDisplayP3).unwrap();
        let view = CanvasView::axis_aligned(Point::new(0.0, 0.0), Point::new(1.0, 1.0)).unwrap();
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
        let cached = crate::canvas::canvas_image::CanvasImage::new(
            Canvas::draw(
                ImagePattern::new(image).with_extend(ImageExtend::Clamp),
                Coverage::all(),
            ),
            view,
            1,
            1,
        )
        .unwrap();
        let backdrop = Canvas::draw(
            Color::new(Srgb, [32_u8, 96, 180], Opacity::<u8>::ONE),
            Coverage::all(),
        );
        let source = Canvas::draw(
            ImagePattern::new(cached).with_extend(ImageExtend::Clamp),
            Coverage::all(),
        );
        let content = SurfaceContent::new(
            backdrop.composite(source, Composition::new(Composite::SourceOver, LinearSrgb)),
            view,
            Color::new(Srgb, [0_u8, 0, 0], Opacity::<u8>::from(0)),
        );
        let expected = linear_srgb.render(&content).unwrap();
        let actual = linear_p3.render(&content).unwrap();
        for (&actual, &expected) in actual.pixels().iter().zip(expected.pixels()) {
            assert!(actual.abs_diff(expected) <= 3, "{actual} != {expected}");
        }
    }

    #[test]
    fn image_surface_renders_into_a_real_icc_output_profile() {
        let size = SurfaceSize {
            width: 2,
            height: 2,
        };
        let Ok(mut surface) = ImageSurface::new(size) else {
            return; // Vulkan may be unavailable on the test host.
        };
        let profile = IccProfile::from_bytes(include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/crates/theoria-color/src/color/fixtures/DisplayP3-v4.icc"
        )))
        .unwrap();
        let view = CanvasView::axis_aligned(Point::new(0.0, 0.0), Point::new(1.0, 1.0)).unwrap();
        let content = SurfaceContent::new(
            Canvas::draw(
                Color::new(Srgb, [255_u8, 0, 0], Opacity::<u8>::ONE),
                Coverage::all(),
            ),
            view,
            Color::new(Srgb, [0_u8, 0, 0], Opacity::<u8>::from(0)),
        );
        let result = surface
            .render_icc(&content, profile, IccRenderingIntent::RelativeColorimetric)
            .unwrap();
        let reference = crate::image::IccRasterImage::from_srgba8(
            &crate::image::Srgba8Image::new(1, 1, vec![255, 0, 0, 255]).unwrap(),
            result.profile().clone(),
            IccRenderingIntent::RelativeColorimetric,
        )
        .unwrap();
        for (actual, expected) in result.pixels()[..4].iter().zip(reference.pixels()) {
            assert!(actual.abs_diff(*expected) <= 3);
        }
        assert_eq!((result.width(), result.height()), (2, 2));
        assert_eq!(result.pixels()[3], 255);
        assert!(result.pixels()[0] < 255);
        assert!(result.pixels()[1] > 0);
    }

    #[test]
    fn image_surface_samples_cmyk_with_alpha_without_preconverting_the_cache() {
        let size = SurfaceSize {
            width: 2,
            height: 2,
        };
        let Ok(mut surface) = ImageSurface::new(size) else {
            return; // Vulkan may be unavailable on the test host.
        };
        let profile = IccProfile::from_bytes(include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/crates/theoria-color/src/color/fixtures/CGATS001Compat-v2-micro.icc"
        )))
        .unwrap();
        let samples = [50, 100, 150, 20, 128];
        let reference = crate::canvas_renderer::raster::convert_icc8_to_linear_rgba16(
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
        let view = CanvasView::axis_aligned(Point::new(0.0, 0.0), Point::new(1.0, 1.0)).unwrap();
        let content = SurfaceContent::new(
            Canvas::draw(
                ImagePattern::new(image).with_extend(ImageExtend::Clamp),
                Coverage::all(),
            ),
            view,
            Color::new(Srgb, [0_u8, 0, 0], Opacity::<u8>::from(0)),
        );
        let rendered = surface.render(&content).unwrap();
        for channel in 0..3 {
            let linear = half::f16::from_bits(reference.samples()[channel]).to_f32();
            let expected =
                (crate::color::encode_srgb(linear).clamp(0.0, 1.0) * 255.0).round() as u8;
            let actual = rendered.pixels()[channel];
            assert!(
                actual.abs_diff(expected) <= 6,
                "channel {channel}: {actual} != {expected}; rgba={:?}",
                &rendered.pixels()[..4]
            );
        }
        assert!(rendered.pixels()[3].abs_diff(128) <= 2);
    }
}
