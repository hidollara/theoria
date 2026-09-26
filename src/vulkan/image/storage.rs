//! Device-local storage and image-view lifetime for one Vulkan image.

use std::error::Error;

use ash::vk;

use crate::vulkan::presenter::RenderDevice;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

pub(in crate::vulkan) struct VulkanImage {
    device: RenderDevice,
    pub(in crate::vulkan) image: vk::Image,
    pub(in crate::vulkan) view: vk::ImageView,
    memory: vk::DeviceMemory,
    pub(in crate::vulkan) format: vk::Format,
    pub(in crate::vulkan) extent: vk::Extent2D,
}

impl VulkanImage {
    pub(in crate::vulkan) fn new(
        device: RenderDevice,
        format: vk::Format,
        extent: vk::Extent2D,
        sample_count: vk::SampleCountFlags,
        usage: vk::ImageUsageFlags,
    ) -> Result<Self> {
        Self::new_with_layers(device, format, extent, sample_count, usage, 1, false)
    }

    pub(in crate::vulkan) fn new_with_layers(
        device: RenderDevice,
        format: vk::Format,
        extent: vk::Extent2D,
        sample_count: vk::SampleCountFlags,
        usage: vk::ImageUsageFlags,
        layers: u32,
        array_view: bool,
    ) -> Result<Self> {
        let raw = device.raw();
        let image = unsafe {
            raw.create_image(
                &vk::ImageCreateInfo::default()
                    .image_type(vk::ImageType::TYPE_2D)
                    .format(format)
                    .extent(vk::Extent3D {
                        width: extent.width,
                        height: extent.height,
                        depth: 1,
                    })
                    .mip_levels(1)
                    .array_layers(layers)
                    .samples(sample_count)
                    .tiling(vk::ImageTiling::OPTIMAL)
                    .usage(usage)
                    .sharing_mode(vk::SharingMode::EXCLUSIVE)
                    .initial_layout(vk::ImageLayout::UNDEFINED),
                None,
            )?
        };
        let requirements = unsafe { raw.get_image_memory_requirements(image) };
        let Some(memory_type_index) = device.find_memory_type(
            requirements.memory_type_bits,
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
        ) else {
            unsafe { raw.destroy_image(image, None) };
            return Err("no device-local Vulkan memory type for image".into());
        };
        let memory = match unsafe {
            raw.allocate_memory(
                &vk::MemoryAllocateInfo::default()
                    .allocation_size(requirements.size)
                    .memory_type_index(memory_type_index),
                None,
            )
        } {
            Ok(memory) => memory,
            Err(error) => {
                unsafe { raw.destroy_image(image, None) };
                return Err(error.into());
            }
        };
        if let Err(error) = unsafe { raw.bind_image_memory(image, memory, 0) } {
            unsafe {
                raw.free_memory(memory, None);
                raw.destroy_image(image, None);
            }
            return Err(error.into());
        }
        let view = match unsafe {
            raw.create_image_view(
                &vk::ImageViewCreateInfo::default()
                    .image(image)
                    .view_type(if array_view {
                        vk::ImageViewType::TYPE_2D_ARRAY
                    } else {
                        vk::ImageViewType::TYPE_2D
                    })
                    .format(format)
                    .subresource_range(
                        vk::ImageSubresourceRange::default()
                            .aspect_mask(vk::ImageAspectFlags::COLOR)
                            .level_count(1)
                            .layer_count(layers),
                    ),
                None,
            )
        } {
            Ok(view) => view,
            Err(error) => {
                unsafe {
                    raw.destroy_image(image, None);
                    raw.free_memory(memory, None);
                }
                return Err(error.into());
            }
        };
        Ok(Self {
            device,
            image,
            view,
            memory,
            format,
            extent,
        })
    }
}

impl Drop for VulkanImage {
    fn drop(&mut self) {
        unsafe {
            self.device.raw().destroy_image_view(self.view, None);
            self.device.raw().destroy_image(self.image, None);
            self.device.raw().free_memory(self.memory, None);
        }
    }
}
