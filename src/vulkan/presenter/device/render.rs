//! Shared logical-device lifetime and rendering capabilities.

use std::sync::Arc;

use ash::vk;

struct Device {
    _entry: ash::Entry,
    instance: ash::Instance,
    physical_device: vk::PhysicalDevice,
    logical: ash::Device,
    memory_properties: vk::PhysicalDeviceMemoryProperties,
    graphics_queue: vk::Queue,
    queue_family_index: u32,
}

impl Drop for Device {
    fn drop(&mut self) {
        unsafe {
            self.logical.destroy_device(None);
            self.instance.destroy_instance(None);
        }
    }
}

#[derive(Clone)]
pub struct RenderDevice(Arc<Device>);

impl RenderDevice {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        entry: ash::Entry,
        instance: ash::Instance,
        physical_device: vk::PhysicalDevice,
        logical: ash::Device,
        memory_properties: vk::PhysicalDeviceMemoryProperties,
        graphics_queue: vk::Queue,
        queue_family_index: u32,
    ) -> Self {
        Self(Arc::new(Device {
            _entry: entry,
            instance,
            physical_device,
            logical,
            memory_properties,
            graphics_queue,
            queue_family_index,
        }))
    }

    pub(crate) fn raw(&self) -> &ash::Device {
        &self.0.logical
    }

    pub(crate) fn graphics_queue(&self) -> vk::Queue {
        self.0.graphics_queue
    }

    pub(crate) fn queue_family_index(&self) -> u32 {
        self.0.queue_family_index
    }

    pub(crate) fn supported_color_sample_counts(&self, format: vk::Format) -> vk::SampleCountFlags {
        unsafe {
            self.0
                .instance
                .get_physical_device_image_format_properties(
                    self.0.physical_device,
                    format,
                    vk::ImageType::TYPE_2D,
                    vk::ImageTiling::OPTIMAL,
                    vk::ImageUsageFlags::COLOR_ATTACHMENT,
                    vk::ImageCreateFlags::empty(),
                )
                .map_or(vk::SampleCountFlags::TYPE_1, |properties| {
                    properties.sample_counts
                })
        }
    }

    pub(crate) fn supports_canvas_working_format(&self, format: vk::Format) -> bool {
        self.format_features(format).contains(
            vk::FormatFeatureFlags::COLOR_ATTACHMENT
                | vk::FormatFeatureFlags::COLOR_ATTACHMENT_BLEND
                | vk::FormatFeatureFlags::SAMPLED_IMAGE
                | vk::FormatFeatureFlags::SAMPLED_IMAGE_FILTER_LINEAR,
        )
    }

    pub(crate) fn supports_filterable_sampled_format(&self, format: vk::Format) -> bool {
        self.format_features(format).contains(
            vk::FormatFeatureFlags::SAMPLED_IMAGE
                | vk::FormatFeatureFlags::SAMPLED_IMAGE_FILTER_LINEAR,
        )
    }

    fn format_features(&self, format: vk::Format) -> vk::FormatFeatureFlags {
        unsafe {
            self.0
                .instance
                .get_physical_device_format_properties(self.0.physical_device, format)
                .optimal_tiling_features
        }
    }

    pub(crate) fn find_memory_type(
        &self,
        allowed_types: u32,
        required: vk::MemoryPropertyFlags,
    ) -> Option<u32> {
        self.0.memory_properties.memory_types[..self.0.memory_properties.memory_type_count as usize]
            .iter()
            .enumerate()
            .find(|(index, memory_type)| {
                allowed_types & (1 << index) != 0 && memory_type.property_flags.contains(required)
            })
            .map(|(index, _)| index as u32)
    }
}
