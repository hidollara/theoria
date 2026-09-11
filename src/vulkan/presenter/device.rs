//! Vulkan device and Surface lifetime used by presentation.

use std::{error::Error, ffi::CStr, sync::Arc};

use ash::{ext, khr, vk};

use crate::target::PresentationTarget;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

const REQUIRED_API_VERSION: u32 = vk::make_api_version(0, 1, 4, 0);
const KHR_SURFACE_MAINTENANCE1_NAME: &CStr = c"VK_KHR_surface_maintenance1";
const KHR_SWAPCHAIN_MAINTENANCE1_NAME: &CStr = c"VK_KHR_swapchain_maintenance1";

struct DeviceInner {
    _entry: ash::Entry,
    instance: ash::Instance,
    physical_device: vk::PhysicalDevice,
    device: ash::Device,
    memory_properties: vk::PhysicalDeviceMemoryProperties,
    graphics_queue: vk::Queue,
    queue_family_index: u32,
}

impl Drop for DeviceInner {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_device(None);
            self.instance.destroy_instance(None);
        }
    }
}

#[derive(Clone)]
pub struct RenderDevice(Arc<DeviceInner>);

impl RenderDevice {
    pub(crate) fn raw(&self) -> &ash::Device {
        &self.0.device
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

/// Owns the Vulkan device capabilities and Surface used by one Presenter.
///
/// The Surface is destroyed before the shared instance held by [`RenderDevice`].
pub(super) struct PresentationDevice {
    pub(super) render: RenderDevice,
    pub(super) surface_loader: khr::surface::Instance,
    pub(super) surface: vk::SurfaceKHR,
    pub(super) physical_device: vk::PhysicalDevice,
    pub(super) present_queue: vk::Queue,
    pub(super) swapchain_loader: khr::swapchain::Device,
    pub(super) swapchain_maintenance1: bool,
}

impl PresentationDevice {
    pub(super) fn new(target: PresentationTarget) -> Result<Self> {
        unsafe {
            let (hwnd, hinstance) = target.win32_handles();
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
            let khr_surface_maintenance1 =
                supports_instance_extension(&entry, KHR_SURFACE_MAINTENANCE1_NAME)?;
            let ext_surface_maintenance1 =
                supports_instance_extension(&entry, ext::surface_maintenance1::NAME)?;
            let mut instance_extensions = vec![
                khr::surface::NAME.as_ptr(),
                khr::win32_surface::NAME.as_ptr(),
            ];
            if khr_surface_maintenance1 {
                instance_extensions.push(KHR_SURFACE_MAINTENANCE1_NAME.as_ptr());
            }
            if ext_surface_maintenance1 {
                instance_extensions.push(ext::surface_maintenance1::NAME.as_ptr());
            }
            let instance_info = vk::InstanceCreateInfo::default()
                .application_info(&app_info)
                .enabled_extension_names(&instance_extensions);
            let instance = entry.create_instance(&instance_info, None)?;

            let win32_surface_loader = khr::win32_surface::Instance::new(&entry, &instance);
            let surface_info = vk::Win32SurfaceCreateInfoKHR::default()
                .hinstance(hinstance)
                .hwnd(hwnd);
            let surface = win32_surface_loader.create_win32_surface(&surface_info, None)?;
            let surface_loader = khr::surface::Instance::new(&entry, &instance);

            let (physical_device, queue_family_index) =
                pick_physical_device(&instance, &surface_loader, surface)?;
            let swapchain_maintenance1_extension = if khr_surface_maintenance1
                && supports_device_extension(
                    &instance,
                    physical_device,
                    KHR_SWAPCHAIN_MAINTENANCE1_NAME,
                )? {
                Some(KHR_SWAPCHAIN_MAINTENANCE1_NAME)
            } else if ext_surface_maintenance1
                && supports_device_extension(
                    &instance,
                    physical_device,
                    ext::swapchain_maintenance1::NAME,
                )?
            {
                Some(ext::swapchain_maintenance1::NAME)
            } else {
                None
            }
            .filter(|_| supports_swapchain_maintenance1(&instance, physical_device));
            let swapchain_maintenance1 = swapchain_maintenance1_extension.is_some();

            let queue_priorities = [1.0];
            let queue_info = vk::DeviceQueueCreateInfo::default()
                .queue_family_index(queue_family_index)
                .queue_priorities(&queue_priorities);
            let mut device_extensions = vec![khr::swapchain::NAME.as_ptr()];
            if let Some(extension) = swapchain_maintenance1_extension {
                device_extensions.push(extension.as_ptr());
            }
            let mut vulkan_13_features = vk::PhysicalDeviceVulkan13Features::default()
                .dynamic_rendering(true)
                .synchronization2(true);
            let mut swapchain_maintenance1_features =
                vk::PhysicalDeviceSwapchainMaintenance1FeaturesEXT::default()
                    .swapchain_maintenance1(true);
            let mut device_info = vk::DeviceCreateInfo::default()
                .queue_create_infos(std::slice::from_ref(&queue_info))
                .enabled_extension_names(&device_extensions)
                .push_next(&mut vulkan_13_features);
            if swapchain_maintenance1 {
                device_info = device_info.push_next(&mut swapchain_maintenance1_features);
            }
            let device = instance.create_device(physical_device, &device_info, None)?;
            let graphics_queue = device.get_device_queue(queue_family_index, 0);
            let memory_properties = instance.get_physical_device_memory_properties(physical_device);
            let swapchain_loader = khr::swapchain::Device::new(&instance, &device);

            let render = RenderDevice(Arc::new(DeviceInner {
                _entry: entry,
                instance,
                physical_device,
                device,
                memory_properties,
                graphics_queue,
                queue_family_index,
            }));

            Ok(Self {
                render,
                surface_loader,
                surface,
                physical_device,
                present_queue: graphics_queue,
                swapchain_loader,
                swapchain_maintenance1,
            })
        }
    }
}

impl Drop for PresentationDevice {
    fn drop(&mut self) {
        unsafe {
            self.surface_loader.destroy_surface(self.surface, None);
        }
    }
}

unsafe fn pick_physical_device(
    instance: &ash::Instance,
    surface_loader: &khr::surface::Instance,
    surface: vk::SurfaceKHR,
) -> Result<(vk::PhysicalDevice, u32)> {
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
            let supports_graphics = family.queue_flags.contains(vk::QueueFlags::GRAPHICS);
            let supports_present = unsafe {
                surface_loader.get_physical_device_surface_support(
                    physical_device,
                    index as u32,
                    surface,
                )?
            };
            if supports_graphics && supports_present {
                candidates.push((score, physical_device, index as u32));
                break;
            }
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
