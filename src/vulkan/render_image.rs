use std::error::Error;

use ash::vk;

use super::{
    image::{ImageCache, SampledImageBinding},
    presenter::RenderDevice,
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

struct ImageAllocation {
    image: vk::Image,
    view: vk::ImageView,
    memory: vk::DeviceMemory,
}

/// A reusable Canvas result. Multisampled rendering is resolved into the
/// single-sample image exposed through `descriptor_set`.
pub(super) struct RenderImage {
    device: RenderDevice,
    color: ImageAllocation,
    resolve: Option<ImageAllocation>,
    binding: Option<SampledImageBinding>,
    pub(super) format: vk::Format,
    pub(super) extent: vk::Extent2D,
    pub(super) sample_count: vk::SampleCountFlags,
}

impl RenderImage {
    pub(super) fn new(
        device: RenderDevice,
        image_cache: &ImageCache,
        format: vk::Format,
        extent: vk::Extent2D,
        sample_count: vk::SampleCountFlags,
    ) -> Result<Self> {
        unsafe {
            let color_usage = if sample_count == vk::SampleCountFlags::TYPE_1 {
                vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED
            } else {
                vk::ImageUsageFlags::COLOR_ATTACHMENT
            };
            let color =
                create_image_allocation(&device, format, extent, sample_count, color_usage)?;
            let resolve = if sample_count == vk::SampleCountFlags::TYPE_1 {
                None
            } else {
                match create_image_allocation(
                    &device,
                    format,
                    extent,
                    vk::SampleCountFlags::TYPE_1,
                    vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED,
                ) {
                    Ok(resolve) => Some(resolve),
                    Err(error) => {
                        destroy_image_allocation(device.raw(), color);
                        return Err(error);
                    }
                }
            };
            let sampled_view = resolve.as_ref().map_or(color.view, |image| image.view);
            let binding = match image_cache
                .bind_sampled_view(sampled_view, vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            {
                Ok(binding) => binding,
                Err(error) => {
                    if let Some(resolve) = resolve {
                        destroy_image_allocation(device.raw(), resolve);
                    }
                    destroy_image_allocation(device.raw(), color);
                    return Err(error);
                }
            };
            Ok(Self {
                device,
                color,
                resolve,
                binding: Some(binding),
                format,
                extent,
                sample_count,
            })
        }
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
        self.binding.as_ref().unwrap().descriptor_set
    }
}

impl Drop for RenderImage {
    fn drop(&mut self) {
        self.binding = None;
        unsafe {
            if let Some(resolve) = self.resolve.take() {
                destroy_image_allocation(self.device.raw(), resolve);
            }
            self.device.raw().destroy_image_view(self.color.view, None);
            self.device.raw().destroy_image(self.color.image, None);
            self.device.raw().free_memory(self.color.memory, None);
        }
    }
}

unsafe fn create_image_allocation(
    device: &RenderDevice,
    format: vk::Format,
    extent: vk::Extent2D,
    sample_count: vk::SampleCountFlags,
    usage: vk::ImageUsageFlags,
) -> Result<ImageAllocation> {
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
                .array_layers(1)
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
        return Err("no device-local Vulkan memory type for render image".into());
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
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(format)
                .subresource_range(
                    vk::ImageSubresourceRange::default()
                        .aspect_mask(vk::ImageAspectFlags::COLOR)
                        .level_count(1)
                        .layer_count(1),
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
    Ok(ImageAllocation {
        image,
        view,
        memory,
    })
}

unsafe fn destroy_image_allocation(device: &ash::Device, allocation: ImageAllocation) {
    unsafe {
        device.destroy_image_view(allocation.view, None);
        device.destroy_image(allocation.image, None);
        device.free_memory(allocation.memory, None);
    }
}
