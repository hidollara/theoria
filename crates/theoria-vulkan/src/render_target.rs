//! A destination for Vulkan Canvas command recording.

use std::marker::PhantomData;

use ash::vk;

use theoria_canvas::canvas::Color;
use theoria_color::{IccProfile, IccRenderingIntent, LinearSrgb};
use theoria_renderer::{OutputAlpha, OutputImage};

/// An acquired destination that a renderer can record commands into.
pub struct RenderTarget<'a> {
    pub(crate) command_buffer: vk::CommandBuffer,
    image_view: vk::ImageView,
    format: vk::Format,
    extent: vk::Extent2D,
    clear_color: Color,
    output: OutputImage,
    _frame: PhantomData<&'a ()>,
}

impl<'a> RenderTarget<'a> {
    pub(crate) fn new(
        command_buffer: vk::CommandBuffer,
        image_view: vk::ImageView,
        format: vk::Format,
        extent: vk::Extent2D,
        clear_color: Color,
    ) -> Self {
        Self {
            command_buffer,
            image_view,
            format,
            extent,
            clear_color,
            output: OutputImage::new(LinearSrgb, OutputAlpha::Premultiplied),
            _frame: PhantomData,
        }
    }

    pub(crate) fn surface(&self) -> (vk::ImageView, vk::Format, vk::Extent2D) {
        (self.image_view, self.format, self.extent)
    }

    pub(crate) fn clear_color(&self) -> Color {
        self.clear_color.clone()
    }

    pub(crate) fn output(&self) -> &OutputImage {
        &self.output
    }

    /// Selects the color and alpha meaning of the destination pixels.
    /// The Vulkan attachment format must be compatible with this description.
    pub fn with_output(mut self, output: OutputImage) -> Self {
        self.output = output;
        self
    }

    /// Encodes the final working image into ICC device RGB coordinates.
    /// The target format must not perform an additional sRGB encoding.
    pub fn with_icc_output(mut self, profile: &IccProfile, intent: IccRenderingIntent) -> Self {
        self.output = OutputImage::new(profile.color_space(intent), OutputAlpha::Premultiplied);
        self
    }
}
