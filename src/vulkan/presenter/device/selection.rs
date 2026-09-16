//! Vulkan capability discovery and physical-device selection.

use std::{error::Error, ffi::CStr, os::raw::c_char};

use ash::{ext, khr, vk};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

pub(super) const REQUIRED_API_VERSION: u32 = vk::make_api_version(0, 1, 4, 0);

const KHR_SURFACE_MAINTENANCE1_NAME: &CStr = c"VK_KHR_surface_maintenance1";
const KHR_SWAPCHAIN_MAINTENANCE1_NAME: &CStr = c"VK_KHR_swapchain_maintenance1";

pub(super) struct InstanceSupport {
    extension_names: Vec<*const c_char>,
    khr_surface_maintenance1: bool,
    ext_surface_maintenance1: bool,
}

impl InstanceSupport {
    pub(super) unsafe fn query(entry: &ash::Entry) -> Result<Self> {
        let loader_version = unsafe {
            entry
                .try_enumerate_instance_version()?
                .unwrap_or(vk::API_VERSION_1_0)
        };
        if loader_version < REQUIRED_API_VERSION {
            return Err(format!(
                "Vulkan 1.4 is required, but the loader only supports {}.{}.{}",
                vk::api_version_major(loader_version),
                vk::api_version_minor(loader_version),
                vk::api_version_patch(loader_version),
            )
            .into());
        }

        let khr_surface_maintenance1 =
            unsafe { supports_instance_extension(entry, KHR_SURFACE_MAINTENANCE1_NAME)? };
        let ext_surface_maintenance1 =
            unsafe { supports_instance_extension(entry, ext::surface_maintenance1::NAME)? };
        let mut extension_names = vec![
            khr::surface::NAME.as_ptr(),
            khr::win32_surface::NAME.as_ptr(),
        ];
        if khr_surface_maintenance1 {
            extension_names.push(KHR_SURFACE_MAINTENANCE1_NAME.as_ptr());
        }
        if ext_surface_maintenance1 {
            extension_names.push(ext::surface_maintenance1::NAME.as_ptr());
        }

        Ok(Self {
            extension_names,
            khr_surface_maintenance1,
            ext_surface_maintenance1,
        })
    }

    pub(super) fn extension_names(&self) -> &[*const c_char] {
        &self.extension_names
    }
}

pub(super) struct DeviceSelection {
    physical_device: vk::PhysicalDevice,
    queue_family_index: u32,
    swapchain_maintenance1_extension: Option<&'static CStr>,
}

impl DeviceSelection {
    pub(super) unsafe fn select(
        instance: &ash::Instance,
        surface_loader: &khr::surface::Instance,
        surface: vk::SurfaceKHR,
        instance_support: &InstanceSupport,
    ) -> Result<Self> {
        let (physical_device, queue_family_index) =
            unsafe { pick_physical_device(instance, surface_loader, surface)? };
        let swapchain_maintenance1_extension = if instance_support.khr_surface_maintenance1
            && unsafe {
                supports_device_extension(
                    instance,
                    physical_device,
                    KHR_SWAPCHAIN_MAINTENANCE1_NAME,
                )?
            } {
            Some(KHR_SWAPCHAIN_MAINTENANCE1_NAME)
        } else if instance_support.ext_surface_maintenance1
            && unsafe {
                supports_device_extension(
                    instance,
                    physical_device,
                    ext::swapchain_maintenance1::NAME,
                )?
            }
        {
            Some(ext::swapchain_maintenance1::NAME)
        } else {
            None
        }
        .filter(|_| unsafe { supports_swapchain_maintenance1(instance, physical_device) });

        Ok(Self {
            physical_device,
            queue_family_index,
            swapchain_maintenance1_extension,
        })
    }

    pub(super) fn physical_device(&self) -> vk::PhysicalDevice {
        self.physical_device
    }

    pub(super) fn queue_family_index(&self) -> u32 {
        self.queue_family_index
    }

    pub(super) fn swapchain_maintenance1_extension(&self) -> Option<&'static CStr> {
        self.swapchain_maintenance1_extension
    }
}

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

unsafe fn pick_physical_device(
    instance: &ash::Instance,
    surface_loader: &khr::surface::Instance,
    surface: vk::SurfaceKHR,
) -> Result<(vk::PhysicalDevice, u32)> {
    let mut candidates = Vec::new();
    for (score, physical_device, queue) in unsafe { render_candidates(instance)? } {
        if unsafe {
            surface_loader.get_physical_device_surface_support(physical_device, queue, surface)?
        } {
            candidates.push((score, physical_device, queue));
        }
    }
    candidates
        .into_iter()
        .max_by_key(|candidate| candidate.0)
        .map(|(_, device, queue)| (device, queue))
        .ok_or_else(|| {
            "no Vulkan 1.4 device with dynamic rendering and synchronization2 can present to this window"
                .into()
        })
}

unsafe fn render_candidates(
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

unsafe fn supports_instance_extension(entry: &ash::Entry, extension: &CStr) -> Result<bool> {
    let properties = unsafe { entry.enumerate_instance_extension_properties(None)? };
    Ok(properties
        .iter()
        .any(|property| unsafe { CStr::from_ptr(property.extension_name.as_ptr()) == extension }))
}

unsafe fn supports_device_extension(
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
    extension: &CStr,
) -> Result<bool> {
    let properties = unsafe { instance.enumerate_device_extension_properties(physical_device)? };
    Ok(properties
        .iter()
        .any(|property| unsafe { CStr::from_ptr(property.extension_name.as_ptr()) == extension }))
}

unsafe fn supports_swapchain_maintenance1(
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
) -> bool {
    let mut maintenance = vk::PhysicalDeviceSwapchainMaintenance1FeaturesEXT::default();
    let mut features = vk::PhysicalDeviceFeatures2::default().push_next(&mut maintenance);
    unsafe { instance.get_physical_device_features2(physical_device, &mut features) };
    maintenance.swapchain_maintenance1 == vk::TRUE
}
