//! Shared logical-device lifetime and rendering capabilities.

use std::{error::Error, sync::Arc};

use ash::vk;

pub(super) mod selection;

use selection::{REQUIRED_API_VERSION, select_render_device};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

struct RenderDeviceConstruction {
    entry: Option<ash::Entry>,
    instance: Option<ash::Instance>,
}

impl RenderDeviceConstruction {
    fn new(entry: ash::Entry, instance: ash::Instance) -> Self {
        Self {
            entry: Some(entry),
            instance: Some(instance),
        }
    }

    fn finish(
        mut self,
        physical_device: vk::PhysicalDevice,
        device: ash::Device,
        memory_properties: vk::PhysicalDeviceMemoryProperties,
        graphics_queue: vk::Queue,
        queue_family_index: u32,
    ) -> RenderDevice {
        RenderDevice::new(
            self.entry.take().expect("entry is present"),
            self.instance.take().expect("instance is present"),
            physical_device,
            device,
            memory_properties,
            graphics_queue,
            queue_family_index,
        )
    }
}

impl Drop for RenderDeviceConstruction {
    fn drop(&mut self) {
        if let Some(instance) = self.instance.as_ref() {
            unsafe { instance.destroy_instance(None) };
        }
    }
}

impl RenderDevice {
    pub(crate) fn independent() -> Result<Self> {
        unsafe {
            let entry = ash::Entry::load()?;
            let loader_version = entry
                .try_enumerate_instance_version()?
                .unwrap_or(vk::API_VERSION_1_0);
            if loader_version < REQUIRED_API_VERSION {
                return Err(format!(
                    "Vulkan 1.4 is required, but the loader only supports {}.{}.{}",
                    vk::api_version_major(loader_version),
                    vk::api_version_minor(loader_version),
                    vk::api_version_patch(loader_version),
                )
                .into());
            }

            let application_name = c"theoria";
            let app_info = vk::ApplicationInfo::default()
                .application_name(application_name)
                .application_version(vk::make_api_version(0, 0, 1, 0))
                .engine_name(application_name)
                .engine_version(vk::make_api_version(0, 0, 1, 0))
                .api_version(REQUIRED_API_VERSION);
            let instance = entry.create_instance(
                &vk::InstanceCreateInfo::default().application_info(&app_info),
                None,
            )?;
            let construction = RenderDeviceConstruction::new(entry, instance);
            let instance = construction.instance.as_ref().expect("instance is present");
            let (physical_device, queue_family_index) = select_render_device(instance)?;
            let queue_priorities = [1.0];
            let queue_info = vk::DeviceQueueCreateInfo::default()
                .queue_family_index(queue_family_index)
                .queue_priorities(&queue_priorities);
            let mut vulkan_13_features = vk::PhysicalDeviceVulkan13Features::default()
                .dynamic_rendering(true)
                .synchronization2(true);
            let device_info = vk::DeviceCreateInfo::default()
                .queue_create_infos(std::slice::from_ref(&queue_info))
                .push_next(&mut vulkan_13_features);
            let device = instance.create_device(physical_device, &device_info, None)?;
            let graphics_queue = device.get_device_queue(queue_family_index, 0);
            let memory_properties = instance.get_physical_device_memory_properties(physical_device);

            Ok(construction.finish(
                physical_device,
                device,
                memory_properties,
                graphics_queue,
                queue_family_index,
            ))
        }
    }
}

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
