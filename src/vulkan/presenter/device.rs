//! Vulkan device and Surface lifetime used by presentation.

use std::error::Error;

use ash::{khr, vk};

use crate::windows::Win32PresentationTarget;

mod render;
mod selection;

pub use render::RenderDevice;
use selection::{DeviceSelection, InstanceSupport, REQUIRED_API_VERSION, select_render_device};

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

struct PresentationDeviceConstruction {
    entry: Option<ash::Entry>,
    instance: Option<ash::Instance>,
    surface_loader: Option<khr::surface::Instance>,
    surface: vk::SurfaceKHR,
}

impl PresentationDeviceConstruction {
    fn new(entry: ash::Entry, instance: ash::Instance) -> Self {
        Self {
            entry: Some(entry),
            instance: Some(instance),
            surface_loader: None,
            surface: vk::SurfaceKHR::null(),
        }
    }

    fn attach_surface(&mut self, surface_loader: khr::surface::Instance, surface: vk::SurfaceKHR) {
        self.surface_loader = Some(surface_loader);
        self.surface = surface;
    }

    #[allow(clippy::too_many_arguments)]
    fn finish(
        mut self,
        physical_device: vk::PhysicalDevice,
        device: ash::Device,
        memory_properties: vk::PhysicalDeviceMemoryProperties,
        graphics_queue: vk::Queue,
        queue_family_index: u32,
        swapchain_loader: khr::swapchain::Device,
        swapchain_maintenance1: bool,
    ) -> PresentationDevice {
        let entry = self.entry.take().expect("entry is present");
        let instance = self.instance.take().expect("instance is present");
        let surface_loader = self
            .surface_loader
            .take()
            .expect("surface loader is present");
        let surface = std::mem::replace(&mut self.surface, vk::SurfaceKHR::null());
        let render = RenderDevice::new(
            entry,
            instance,
            physical_device,
            device,
            memory_properties,
            graphics_queue,
            queue_family_index,
        );
        PresentationDevice {
            render,
            surface_loader,
            surface,
            physical_device,
            present_queue: graphics_queue,
            swapchain_loader,
            swapchain_maintenance1,
        }
    }
}

impl Drop for PresentationDeviceConstruction {
    fn drop(&mut self) {
        unsafe {
            if self.surface != vk::SurfaceKHR::null()
                && let Some(surface_loader) = self.surface_loader.as_ref()
            {
                surface_loader.destroy_surface(self.surface, None);
            }
            if let Some(instance) = self.instance.as_ref() {
                instance.destroy_instance(None);
            }
        }
    }
}

/// Owns the Vulkan device capabilities and Surface used by one Win32Presenter.
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
    pub(super) fn new(target: Win32PresentationTarget) -> Result<Self> {
        unsafe {
            let (hwnd, hinstance) = target.raw_handles();
            let entry = ash::Entry::load()?;
            let instance_support = InstanceSupport::query(&entry)?;

            let application_name = c"theoria";
            let app_info = vk::ApplicationInfo::default()
                .application_name(application_name)
                .application_version(vk::make_api_version(0, 0, 1, 0))
                .engine_name(application_name)
                .engine_version(vk::make_api_version(0, 0, 1, 0))
                .api_version(REQUIRED_API_VERSION);
            let instance_info = vk::InstanceCreateInfo::default()
                .application_info(&app_info)
                .enabled_extension_names(instance_support.extension_names());
            let instance = entry.create_instance(&instance_info, None)?;
            let mut construction = PresentationDeviceConstruction::new(entry, instance);

            let entry = construction.entry.as_ref().unwrap();
            let instance = construction.instance.as_ref().unwrap();
            let win32_surface_loader = khr::win32_surface::Instance::new(entry, instance);
            let surface_info = vk::Win32SurfaceCreateInfoKHR::default()
                .hinstance(hinstance)
                .hwnd(hwnd);
            let surface = win32_surface_loader.create_win32_surface(&surface_info, None)?;
            let surface_loader = khr::surface::Instance::new(entry, instance);
            construction.attach_surface(surface_loader, surface);
            let instance = construction.instance.as_ref().unwrap();
            let surface_loader = construction.surface_loader.as_ref().unwrap();

            let selected =
                DeviceSelection::select(instance, surface_loader, surface, &instance_support)?;
            let physical_device = selected.physical_device();
            let queue_family_index = selected.queue_family_index();
            let swapchain_maintenance1 = selected.swapchain_maintenance1_extension().is_some();

            let queue_priorities = [1.0];
            let queue_info = vk::DeviceQueueCreateInfo::default()
                .queue_family_index(queue_family_index)
                .queue_priorities(&queue_priorities);
            let mut device_extensions = vec![khr::swapchain::NAME.as_ptr()];
            if let Some(extension) = selected.swapchain_maintenance1_extension() {
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
            let swapchain_loader = khr::swapchain::Device::new(instance, &device);

            Ok(construction.finish(
                physical_device,
                device,
                memory_properties,
                graphics_queue,
                queue_family_index,
                swapchain_loader,
                swapchain_maintenance1,
            ))
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
