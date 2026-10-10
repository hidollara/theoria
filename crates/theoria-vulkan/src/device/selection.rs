//! Rendering capability discovery independent of presentation.

use std::error::Error;

use ash::vk;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

pub(crate) const REQUIRED_API_VERSION: u32 = vk::make_api_version(0, 1, 4, 0);

pub(super) unsafe fn select_render_device(
    instance: &ash::Instance,
) -> Result<(vk::PhysicalDevice, u32)> {
    unsafe { render_candidates(instance)? }
        .into_iter()
        .max_by_key(|candidate| candidate.0)
        .map(|(_, device, queue)| (device, queue))
        .ok_or_else(|| {
            "no Vulkan 1.4 graphics device with dynamic rendering and synchronization2".into()
        })
}

pub(crate) unsafe fn render_candidates(
    instance: &ash::Instance,
) -> Result<Vec<(u8, vk::PhysicalDevice, u32)>> {
    let mut candidates = Vec::new();
    for physical_device in unsafe { instance.enumerate_physical_devices()? } {
        let properties = unsafe { instance.get_physical_device_properties(physical_device) };
        if properties.api_version < REQUIRED_API_VERSION {
            continue;
        }
        let mut vulkan_13_features = vk::PhysicalDeviceVulkan13Features::default();
        let mut features =
            vk::PhysicalDeviceFeatures2::default().push_next(&mut vulkan_13_features);
        unsafe { instance.get_physical_device_features2(physical_device, &mut features) };
        if vulkan_13_features.dynamic_rendering == vk::FALSE
            || vulkan_13_features.synchronization2 == vk::FALSE
        {
            continue;
        }
        let score = match properties.device_type {
            vk::PhysicalDeviceType::DISCRETE_GPU => 2,
            vk::PhysicalDeviceType::INTEGRATED_GPU => 1,
            _ => 0,
        };
        let queue_families =
            unsafe { instance.get_physical_device_queue_family_properties(physical_device) };
        for (index, family) in queue_families.iter().enumerate() {
            if family.queue_flags.contains(vk::QueueFlags::GRAPHICS) {
                candidates.push((score, physical_device, index as u32));
            }
        }
    }
    Ok(candidates)
}
