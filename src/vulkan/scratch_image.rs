use std::error::Error;

use ash::vk;

use super::{
    device::RenderDevice,
    image::{ImageCache, SampledImageBinding, VulkanImage},
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

/// A reusable renderer-owned intermediate. Multisampled rendering is resolved
/// into the single-sample image exposed through `descriptor_set`.
pub(super) struct ScratchImage {
    // Fields drop in declaration order, so descriptor references disappear
    // before the image views they name.
    binding: SampledImageBinding,
    color: VulkanImage,
    resolve: Option<VulkanImage>,
    pub(super) format: vk::Format,
    pub(super) extent: vk::Extent2D,
    pub(super) sample_count: vk::SampleCountFlags,
}

impl ScratchImage {
    pub(super) fn new(
        device: RenderDevice,
        image_cache: &ImageCache,
        format: vk::Format,
        extent: vk::Extent2D,
        sample_count: vk::SampleCountFlags,
    ) -> Result<Self> {
        let color_usage = if sample_count == vk::SampleCountFlags::TYPE_1 {
            vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED
        } else {
            vk::ImageUsageFlags::COLOR_ATTACHMENT
        };
        let color = VulkanImage::new(device.clone(), format, extent, sample_count, color_usage)?;
        let resolve = if sample_count == vk::SampleCountFlags::TYPE_1 {
            None
        } else {
            Some(VulkanImage::new(
                device,
                format,
                extent,
                vk::SampleCountFlags::TYPE_1,
                vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED,
            )?)
        };
        let sampled_view = resolve.as_ref().map_or(color.view, |image| image.view);
        let binding = image_cache
            .bind_sampled_view(sampled_view, vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)?;
        Ok(Self {
            binding,
            color,
            resolve,
            format,
            extent,
            sample_count,
        })
    }

    pub(super) fn attachment_image(&self) -> vk::Image {
        self.color.image
    }

    pub(super) fn attachment_view(&self) -> vk::ImageView {
        self.color.view
    }

    pub(super) fn sampled_image(&self) -> vk::Image {
        self.resolve
            .as_ref()
            .map_or(self.color.image, |image| image.image)
    }

    pub(super) fn sampled_view(&self) -> vk::ImageView {
        self.resolve
            .as_ref()
            .map_or(self.color.view, |image| image.view)
    }

    pub(super) fn is_multisampled(&self) -> bool {
        self.resolve.is_some()
    }

    pub(super) fn descriptor_set(&self) -> vk::DescriptorSet {
        self.binding.descriptor_set
    }
}
