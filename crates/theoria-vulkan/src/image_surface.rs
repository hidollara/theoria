//! Offscreen Canvas evaluation into CPU-readable sRGBA8 images.

use std::error::Error;

use ash::vk;

use crate::{
    canvas_renderer::CanvasRenderer, device::RenderDevice, host_buffer::HostBuffer,
    image::VulkanImage, image_layout::transition_image, one_shot::OneShotCommands,
    render_target::RenderTarget,
};
use theoria_canvas::image::{IccRasterImage, Srgba8Image};
use theoria_color::{ColorSpace, IccProfile, IccRenderingIntent, LinearSrgb};
use theoria_renderer::{OutputAlpha, OutputImage, SurfaceContent, SurfaceSize, Viewport};

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
        working_space: impl Into<ColorSpace>,
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
            renderer: CanvasRenderer::new_with_working_space(device, working_space),
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
        let pixels = self.render_raw(content, None)?;
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
        let pixels = self.render_raw(content, Some((&profile, intent)))?;
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
        let output = match output_profile {
            Some((profile, intent)) => {
                OutputImage::new(profile.color_space(intent), OutputAlpha::Straight)
            }
            None => OutputImage::new(theoria_color::Srgb, OutputAlpha::Straight),
        };
        let target = target.with_output(output);
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
    use ash::vk;
    use theoria_canvas::canvas::{
        Canvas, CanvasView, Color, Composite, Composition, Coverage, ImageExtend, ImagePattern,
        Opacity,
    };
    use theoria_canvas::geometry::Point;
    use theoria_color::{IccProfile, IccRenderingIntent, LinearDisplayP3, LinearSrgb, Srgb};
    use theoria_renderer::{SurfaceContent, SurfaceSize};

    fn test_surface(size: SurfaceSize) -> Option<ImageSurface> {
        match ImageSurface::new(size) {
            Ok(surface) => Some(surface),
            Err(error) => {
                eprintln!("skipping Vulkan render test: no usable render device: {error}");
                None
            }
        }
    }

    #[test]
    fn clipped_child_canvas_preserves_its_position_and_size() {
        let Some(mut surface) = test_surface(SurfaceSize {
            width: 64,
            height: 64,
        }) else {
            return;
        };
        let rect = |x0, y0, x1, y1| {
            Coverage::fill(theoria_canvas::path::Path::polygon([
                Point::new(x0, y0),
                Point::new(x1, y0),
                Point::new(x1, y1),
                Point::new(x0, y1),
            ]))
        };
        let child = Canvas::draw(
            Color::new(Srgb, [255_u8, 0, 0], Opacity::<u8>::ONE),
            rect(24.0, 24.0, 56.0, 56.0),
        );
        let canvas = Canvas::draw(child, rect(0.0, 0.0, 48.0, 48.0));
        let view = CanvasView::axis_aligned(Point::new(0.0, 0.0), Point::new(64.0, 64.0)).unwrap();
        let cached =
            theoria_canvas::canvas::CanvasImage::new(canvas.clone(), view, 64, 64).unwrap();
        // Both the multisampled working composite and the single-sampled
        // CanvasImage output must preserve coordinates.
        for canvas in [canvas, Canvas::draw(cached, Coverage::all())] {
            let image = surface
                .render(&SurfaceContent::new(
                    canvas,
                    view,
                    Color::new(Srgb, [0_u8, 0, 0], Opacity::<u8>::ONE),
                ))
                .unwrap();
            for (x, y, expected) in [
                (16, 16, [0, 0, 0, 255]),
                (28, 28, [255, 0, 0, 255]),
                (44, 44, [255, 0, 0, 255]),
                (52, 52, [0, 0, 0, 255]),
            ] {
                let offset = (y * 64 + x) * 4;
                assert_eq!(
                    &image.pixels()[offset..offset + 4],
                    &expected,
                    "at ({x}, {y})"
                );
            }
        }
    }

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
    fn invalid_custom_blend_is_a_vulkan_render_error() {
        let size = SurfaceSize {
            width: 2,
            height: 2,
        };
        let Some(mut surface) = test_surface(size) else {
            return;
        };
        let color = theoria_canvas::Color::new(Srgb, [255_u8, 0, 0], Opacity::<u8>::ONE);
        let draw = Canvas::draw(color.clone(), Coverage::all());
        let view = CanvasView::axis_aligned(Point::new(0.0, 0.0), Point::new(1.0, 1.0)).unwrap();
        for definition in [
            theoria_canvas::WgslBlend::new("return missing(source);"),
            theoria_canvas::WgslBlend::new("return source;").with_helpers(
                "fn working_to_composite(value: vec3<f32>) -> vec3<f32> { return value; }",
            ),
        ] {
            let canvas = draw.clone().composite(
                draw.clone(),
                Composition::new(Composite::SourceOver, LinearSrgb)
                    .with_blend(theoria_canvas::BlendShader::wgsl(definition), Srgb),
            );
            let content = SurfaceContent::new(canvas, view, color.clone());
            let result = surface.render(&content);
            assert!(result.is_err(), "invalid blends must return a render error");
        }
        // A rejected definition must not prevent a subsequent valid render.
        let image = surface
            .render(&SurfaceContent::new(draw, view, color))
            .unwrap();
        assert_eq!(&image.pixels()[..4], &[255, 0, 0, 255]);
    }

    #[test]
    fn selected_working_space_preserves_srgba_readback() {
        let size = SurfaceSize {
            width: 2,
            height: 2,
        };
        let Some(mut linear_srgb) = test_surface(size) else {
            return;
        };
        let mut linear_p3 = ImageSurface::new_with_working_space(size, LinearDisplayP3).unwrap();
        let view = CanvasView::axis_aligned(Point::new(0.0, 0.0), Point::new(1.0, 1.0)).unwrap();
        let profile = IccProfile::from_bytes(include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../theoria-color/src/color/fixtures/DisplayP3-v4.icc"
        )))
        .unwrap();
        let image = theoria_canvas::canvas::Image::from_icc_u8(
            1,
            1,
            vec![200, 100, 50],
            profile,
            IccRenderingIntent::RelativeColorimetric,
            false,
        )
        .unwrap();
        let cached = theoria_canvas::canvas::canvas_image::CanvasImage::new(
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
    fn output_composites_before_encoding_and_keeps_straight_alpha() {
        let size = SurfaceSize {
            width: 1,
            height: 1,
        };
        let Some(mut surface) = test_surface(size) else {
            return;
        };
        let view = CanvasView::axis_aligned(Point::new(0.0, 0.0), Point::new(1.0, 1.0)).unwrap();
        let content = SurfaceContent::new(
            Canvas::draw(
                Color::new(Srgb, [255_u8, 0, 0], Opacity::<u8>::from(128)),
                Coverage::all(),
            ),
            view,
            Color::new(Srgb, [0_u8, 0, 255], Opacity::<u8>::from(128)),
        );
        let result = surface.render(&content).unwrap();
        let pixel = &result.pixels()[..4];
        for (&actual, expected) in pixel.iter().zip([213_u8, 0, 156, 192]) {
            assert!(actual.abs_diff(expected) <= 3, "rgba={pixel:?}");
        }
    }

    #[test]
    fn low_alpha_color_is_unassociated_before_rgba8_quantization() {
        let size = SurfaceSize {
            width: 1,
            height: 1,
        };
        let Some(mut surface) = test_surface(size) else {
            return;
        };
        if !surface
            .renderer
            .device()
            .supports_canvas_working_format(vk::Format::R16G16B16A16_SFLOAT)
        {
            return;
        }
        let view = CanvasView::axis_aligned(Point::new(0.0, 0.0), Point::new(1.0, 1.0)).unwrap();
        let content = SurfaceContent::new(
            Canvas::draw(
                Color::new(Srgb, [255_u8, 0, 0], Opacity::<u8>::from(1)),
                Coverage::all(),
            ),
            view,
            Color::new(Srgb, [0_u8, 0, 0], Opacity::<u8>::from(0)),
        );
        let image = surface.render(&content).unwrap();
        let pixel = &image.pixels()[..4];
        assert!(pixel[0] >= 250, "rgba={pixel:?}");
        assert_eq!(pixel[3], 1, "rgba={pixel:?}");
    }

    #[test]
    fn icc_output_does_not_quantize_premultiplied_rgb_before_unassociation() {
        let size = SurfaceSize {
            width: 1,
            height: 1,
        };
        let Some(mut surface) = test_surface(size) else {
            return;
        };
        let profile = IccProfile::from_bytes(include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../theoria-color/src/color/fixtures/DisplayP3-v4.icc"
        )))
        .unwrap();
        let view = CanvasView::axis_aligned(Point::new(0.0, 0.0), Point::new(1.0, 1.0)).unwrap();
        let content = SurfaceContent::new(
            Canvas::draw(
                Color::new(Srgb, [255_u8, 0, 0], Opacity::<u8>::from(128)),
                Coverage::all(),
            ),
            view,
            Color::new(Srgb, [0_u8, 0, 0], Opacity::<u8>::from(0)),
        );
        let image = surface
            .render_icc(
                &content,
                profile.clone(),
                IccRenderingIntent::RelativeColorimetric,
            )
            .unwrap();
        let reference = theoria_canvas::image::IccRasterImage::from_srgba8(
            &theoria_canvas::image::Srgba8Image::new(1, 1, vec![255, 0, 0, 128]).unwrap(),
            profile,
            IccRenderingIntent::RelativeColorimetric,
        )
        .unwrap();
        for (&actual, &expected) in image.pixels().iter().zip(reference.pixels()) {
            assert!(
                actual.abs_diff(expected) <= 3,
                "actual={:?} expected={:?}",
                image.pixels(),
                reference.pixels()
            );
        }
    }

    #[test]
    fn image_surface_renders_into_a_real_icc_output_profile() {
        let size = SurfaceSize {
            width: 2,
            height: 2,
        };
        let Some(mut surface) = test_surface(size) else {
            return;
        };
        let profile = IccProfile::from_bytes(include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../theoria-color/src/color/fixtures/DisplayP3-v4.icc"
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
        let reference = theoria_canvas::image::IccRasterImage::from_srgba8(
            &theoria_canvas::image::Srgba8Image::new(1, 1, vec![255, 0, 0, 255]).unwrap(),
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
        let Some(mut surface) = test_surface(size) else {
            return;
        };
        let profile = IccProfile::from_bytes(include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../theoria-color/src/color/fixtures/CGATS001Compat-v2-micro.icc"
        )))
        .unwrap();
        let samples = [50, 100, 150, 20, 128];
        let reference = crate::test_support::convert_icc8_to_linear_rgba16(
            1,
            &samples,
            &profile,
            IccRenderingIntent::Perceptual,
            true,
        )
        .unwrap();
        let image = theoria_canvas::canvas::Image::from_icc_u8(
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
            let linear = f64::from(linear);
            let magnitude = linear.abs();
            let encoded = if magnitude <= 0.003_130_8 {
                12.92 * linear
            } else {
                linear.signum() * (1.055 * magnitude.powf(1.0 / 2.4) - 0.055)
            };
            let expected = (encoded.clamp(0.0, 1.0) * 255.0).round() as u8;
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
