use std::error::Error;

use ash::vk;

use super::{
    image::{ImageCache, SampledImageBinding},
    presenter::RenderDevice,
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

pub(super) struct RenderImage {
    device: RenderDevice,
    pub(super) image: vk::Image,
    pub(super) view: vk::ImageView,
    memory: vk::DeviceMemory,
    binding: Option<SampledImageBinding>,
    pub(super) format: vk::Format,
    pub(super) extent: vk::Extent2D,
}

impl RenderImage {
    pub(super) fn new(
        device: RenderDevice,
        image_cache: &ImageCache,
        format: vk::Format,
        extent: vk::Extent2D,
    ) -> Result<Self> {
        unsafe {
            let raw = device.raw();
            let image = raw.create_image(
                &vk::ImageCreateInfo::default()
                    .image_type(vk::ImageType::TYPE_2D)
                    .format(format)
                    .extent(vk::Extent3D {
                        width: extent.width,
                        height: extent.height,
                        depth: 1,
                    })
                    .mip_levels(1)
                    .array_layers(1)
                    .samples(vk::SampleCountFlags::TYPE_1)
                    .tiling(vk::ImageTiling::OPTIMAL)
                    .usage(vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED)
                    .sharing_mode(vk::SharingMode::EXCLUSIVE)
                    .initial_layout(vk::ImageLayout::UNDEFINED),
                None,
            )?;
            let requirements = raw.get_image_memory_requirements(image);
            let Some(memory_type_index) = device.find_memory_type(
                requirements.memory_type_bits,
                vk::MemoryPropertyFlags::DEVICE_LOCAL,
            ) else {
                raw.destroy_image(image, None);
                return Err("no device-local Vulkan memory type for render image".into());
            };
            let memory = match raw.allocate_memory(
                &vk::MemoryAllocateInfo::default()
                    .allocation_size(requirements.size)
                    .memory_type_index(memory_type_index),
                None,
            ) {
                Ok(memory) => memory,
                Err(error) => {
                    raw.destroy_image(image, None);
                    return Err(error.into());
                }
            };
            if let Err(error) = raw.bind_image_memory(image, memory, 0) {
                raw.free_memory(memory, None);
                raw.destroy_image(image, None);
                return Err(error.into());
            }
            let view = match raw.create_image_view(
                &vk::ImageViewCreateInfo::default()
                    .image(image)
                    .view_type(vk::ImageViewType::TYPE_2D)
                    .format(format)
                    .subresource_range(
                        vk::ImageSubresourceRange::default()
                            .aspect_mask(vk::ImageAspectFlags::COLOR)
                            .level_count(1)
                            .layer_count(1),
                    ),
                None,
            ) {
                Ok(view) => view,
                Err(error) => {
                    raw.destroy_image(image, None);
                    raw.free_memory(memory, None);
                    return Err(error.into());
                }
            };
            let binding = match image_cache
                .bind_sampled_view(view, vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            {
                Ok(binding) => binding,
                Err(error) => {
                    raw.destroy_image_view(view, None);
                    raw.destroy_image(image, None);
                    raw.free_memory(memory, None);
                    return Err(error);
                }
            };
            Ok(Self {
                device,
                image,
                view,
                memory,
                binding: Some(binding),
                format,
                extent,
            })
        }
    }

    pub(super) fn descriptor_set(&self) -> vk::DescriptorSet {
        self.binding.as_ref().unwrap().descriptor_set
    }
}

impl Drop for RenderImage {
    fn drop(&mut self) {
        self.binding = None;
        unsafe {
            self.device.raw().destroy_image_view(self.view, None);
            self.device.raw().destroy_image(self.image, None);
            self.device.raw().free_memory(self.memory, None);
        }
    }
}
