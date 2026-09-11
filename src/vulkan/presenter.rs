use std::{
    error::Error,
    ffi::CStr,
    sync::Arc,
    time::{Duration, Instant},
};

use ash::{ext, khr, vk};

use crate::{color::Color, surface::SurfaceSize, target::PresentationTarget};

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

struct SwapchainResources {
    handle: vk::SwapchainKHR,
    image_views: Vec<vk::ImageView>,
    render_targets: Vec<SurfaceRenderTarget>,
    command_buffers: Vec<vk::CommandBuffer>,
    render_finished: Vec<vk::Semaphore>,
    presentation_release_fences: Vec<vk::Fence>,
    submitted_for_presentation: Vec<bool>,
}

#[derive(Clone, Copy)]
struct SurfaceRenderTarget {
    image: vk::Image,
    image_view: vk::ImageView,
    format: vk::Format,
    extent: vk::Extent2D,
}

/// An acquired destination that a renderer can record commands into.
pub struct RenderTarget<'a> {
    pub(crate) command_buffer: vk::CommandBuffer,
    surface: SurfaceRenderTarget,
    clear_color: Color,
    _frame: std::marker::PhantomData<&'a ()>,
}

impl RenderTarget<'_> {
    pub(crate) fn surface(&self) -> (vk::ImageView, vk::Format, vk::Extent2D) {
        (
            self.surface.image_view,
            self.surface.format,
            self.surface.extent,
        )
    }

    pub(crate) fn clear_color(&self) -> Color {
        self.clear_color
    }
}

/// Owns swapchain acquisition, synchronization, and final presentation.
pub struct Presenter {
    device: RenderDevice,
    surface_loader: khr::surface::Instance,
    surface: vk::SurfaceKHR,
    physical_device: vk::PhysicalDevice,
    graphics_queue: vk::Queue,
    present_queue: vk::Queue,
    queue_family_index: u32,
    swapchain_loader: khr::swapchain::Device,
    swapchain_maintenance1: bool,
    command_pool: vk::CommandPool,
    image_available: vk::Semaphore,
    frame_finished: vk::Fence,
    requested_size: SurfaceSize,
    swapchain_needs_recreation: bool,
    swapchain: Option<SwapchainResources>,
    // Keeping at most one retired generation bounds presentation memory. If it
    // is still in use, a newer resize request waits here without blocking the
    // native event loop and replaces only the latest requested size.
    retired_swapchain: Option<SwapchainResources>,
}

impl Presenter {
    pub fn new(target: PresentationTarget) -> Result<Self> {
        unsafe {
            let size = target.size();
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
            let present_queue = graphics_queue;
            let memory_properties = instance.get_physical_device_memory_properties(physical_device);
            let swapchain_loader = khr::swapchain::Device::new(&instance, &device);

            let device = RenderDevice(Arc::new(DeviceInner {
                _entry: entry,
                instance,
                physical_device,
                device,
                memory_properties,
                graphics_queue,
                queue_family_index,
            }));

            let command_pool_info = vk::CommandPoolCreateInfo::default()
                .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
                .queue_family_index(queue_family_index);
            let command_pool = device.raw().create_command_pool(&command_pool_info, None)?;
            let semaphore_info = vk::SemaphoreCreateInfo::default();
            let image_available = device.raw().create_semaphore(&semaphore_info, None)?;
            let fence_info = vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED);
            let frame_finished = device.raw().create_fence(&fence_info, None)?;

            let mut renderer = Self {
                device,
                surface_loader,
                surface,
                physical_device,
                graphics_queue,
                present_queue,
                queue_family_index,
                swapchain_loader,
                swapchain_maintenance1,
                command_pool,
                image_available,
                frame_finished,
                requested_size: size,
                swapchain_needs_recreation: true,
                swapchain: None,
                retired_swapchain: None,
            };
            renderer.recreate_swapchain()?;
            Ok(renderer)
        }
    }

    pub fn device(&self) -> RenderDevice {
        self.device.clone()
    }

    pub fn resize(&mut self, size: SurfaceSize) -> Result<()> {
        if self.requested_size != size {
            self.requested_size = size;
            self.swapchain_needs_recreation = true;
        }
        Ok(())
    }

    /// Attempts to acquire a renderable Swapchain image within `timeout`.
    ///
    /// `None` means that no image is ready yet, the surface has no drawable
    /// extent, or an obsolete Swapchain is being replaced. The caller chooses
    /// whether and how long to wait, allowing native event processing to remain
    /// responsive. The timeout is shared between waiting for the previous GPU
    /// frame, releasing one retired Swapchain generation, and acquiring a
    /// presentation image. A newer replacement is deferred while that retired
    /// generation remains in use.
    pub fn begin_frame(&mut self, timeout: Duration) -> Result<Option<PresentFrame<'_>>> {
        if self.requested_size.width == 0 || self.requested_size.height == 0 {
            return Ok(None);
        }

        unsafe {
            let started_at = Instant::now();
            match self.device.raw().wait_for_fences(
                &[self.frame_finished],
                true,
                timeout_nanoseconds(timeout),
            ) {
                Ok(()) => {}
                Err(vk::Result::TIMEOUT) => return Ok(None),
                Err(error) => return Err(error.into()),
            }

            let remaining = timeout.saturating_sub(started_at.elapsed());
            self.reap_retired_swapchain(remaining)?;
            if self.swapchain_needs_recreation || self.swapchain.is_none() {
                if self.retired_swapchain.is_some() {
                    return Ok(None);
                }
                self.recreate_swapchain()?;
            }

            let resources = self.swapchain.as_ref().expect("swapchain is available");
            let remaining = timeout.saturating_sub(started_at.elapsed());
            let (image_index, acquire_suboptimal) = match self.swapchain_loader.acquire_next_image(
                resources.handle,
                timeout_nanoseconds(remaining),
                self.image_available,
                vk::Fence::null(),
            ) {
                Ok(result) => result,
                Err(vk::Result::NOT_READY | vk::Result::TIMEOUT) => return Ok(None),
                Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                    self.swapchain_needs_recreation = true;
                    return Ok(None);
                }
                Err(error) => return Err(error.into()),
            };

            let command_buffer = resources.command_buffers[image_index as usize];
            self.device
                .raw()
                .reset_command_buffer(command_buffer, vk::CommandBufferResetFlags::empty())?;
            self.device
                .raw()
                .begin_command_buffer(command_buffer, &vk::CommandBufferBeginInfo::default())?;
            transition_image(
                self.device.raw(),
                command_buffer,
                resources.render_targets[image_index as usize].image,
                vk::ImageLayout::UNDEFINED,
                vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                vk::PipelineStageFlags2::NONE,
                vk::AccessFlags2::NONE,
                vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
            );
            Ok(Some(PresentFrame {
                presenter: self,
                image_index,
                command_buffer,
                acquire_suboptimal,
            }))
        }
    }

    fn recreate_swapchain(&mut self) -> Result<()> {
        if self.requested_size.width == 0 || self.requested_size.height == 0 {
            return Ok(());
        }
        unsafe {
            if self.swapchain.is_some() && !self.swapchain_maintenance1 {
                // Without presentation fences, Vulkan provides no portable way
                // to prove that a retired Swapchain is no longer in use.
                self.device.raw().device_wait_idle()?;
            }

            let old_swapchain = self
                .swapchain
                .as_ref()
                .map_or(vk::SwapchainKHR::null(), |resources| resources.handle);
            let new_swapchain = create_swapchain_resources(
                &self.surface_loader,
                self.surface,
                self.physical_device,
                self.device.raw(),
                &self.swapchain_loader,
                self.command_pool,
                self.queue_family_index,
                self.requested_size,
                old_swapchain,
                self.swapchain_maintenance1,
            )?;

            if let Some(old) = self.swapchain.replace(new_swapchain) {
                if self.swapchain_maintenance1 {
                    debug_assert!(self.retired_swapchain.is_none());
                    self.retired_swapchain = Some(old);
                } else {
                    destroy_swapchain_resources(
                        self.device.raw(),
                        &self.swapchain_loader,
                        self.command_pool,
                        old,
                    );
                }
            }
            self.swapchain_needs_recreation = false;
        }
        Ok(())
    }

    pub(crate) fn has_retired_swapchain(&self) -> bool {
        self.retired_swapchain.is_some()
    }

    /// Waits at most `timeout` for retired presentation resources and destroys
    /// the generation once its final presentation has released its resources.
    pub(crate) fn reap_retired_swapchain(&mut self, timeout: Duration) -> Result<()> {
        if !timeout.is_zero()
            && let Some(resources) = self.retired_swapchain.as_ref()
        {
            for (&submitted, &fence) in resources
                .submitted_for_presentation
                .iter()
                .zip(&resources.presentation_release_fences)
            {
                if submitted && !unsafe { self.device.raw().get_fence_status(fence)? } {
                    unsafe {
                        match self.device.raw().wait_for_fences(
                            &[fence],
                            true,
                            timeout_nanoseconds(timeout),
                        ) {
                            Ok(()) | Err(vk::Result::TIMEOUT) => {}
                            Err(error) => return Err(error.into()),
                        }
                    }
                    break;
                }
            }
        }

        let ready = if let Some(resources) = self.retired_swapchain.as_ref() {
            unsafe { presentation_resources_released(self.device.raw(), resources)? }
        } else {
            false
        };
        if ready {
            let resources = self
                .retired_swapchain
                .take()
                .expect("a ready retired Swapchain is present");
            unsafe {
                destroy_swapchain_resources(
                    self.device.raw(),
                    &self.swapchain_loader,
                    self.command_pool,
                    resources,
                );
            }
        }
        Ok(())
    }
}

pub struct PresentFrame<'a> {
    presenter: &'a mut Presenter,
    image_index: u32,
    command_buffer: vk::CommandBuffer,
    acquire_suboptimal: bool,
}

/// Result of handing a rendered frame to the presentation engine.
#[must_use = "an invalidated Swapchain still needs a replacement frame"]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresentOutcome {
    /// The frame was accepted by the current Swapchain.
    Presented,
    /// The current Swapchain became obsolete and must be replaced next frame.
    SwapchainInvalidated,
}

impl PresentFrame<'_> {
    pub fn render_target(&self, clear_color: Color) -> RenderTarget<'_> {
        let resources = self.presenter.swapchain.as_ref().unwrap();
        RenderTarget {
            command_buffer: self.command_buffer,
            surface: resources.render_targets[self.image_index as usize],
            clear_color,
            _frame: std::marker::PhantomData,
        }
    }

    /// Submits the recorded frame and presents it.
    ///
    /// [`PresentOutcome::SwapchainInvalidated`] means that the submitted image
    /// belonged to a Swapchain which must be replaced before the next frame.
    pub fn present(self) -> Result<PresentOutcome> {
        unsafe {
            let presenter = self.presenter;
            let image_index = self.image_index as usize;
            let resources = presenter.swapchain.as_ref().unwrap();
            let image = resources.render_targets[image_index].image;
            let render_finished = resources.render_finished[image_index];
            transition_image(
                presenter.device.raw(),
                self.command_buffer,
                image,
                vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                vk::ImageLayout::PRESENT_SRC_KHR,
                vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
                vk::PipelineStageFlags2::NONE,
                vk::AccessFlags2::NONE,
            );
            presenter
                .device
                .raw()
                .end_command_buffer(self.command_buffer)?;
            presenter
                .device
                .raw()
                .reset_fences(&[presenter.frame_finished])?;
            let wait_semaphores = [presenter.image_available];
            let signal_semaphores = [render_finished];
            let wait_stages = [vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
            let command_buffers = [self.command_buffer];
            let submit_info = vk::SubmitInfo::default()
                .wait_semaphores(&wait_semaphores)
                .wait_dst_stage_mask(&wait_stages)
                .command_buffers(&command_buffers)
                .signal_semaphores(&signal_semaphores);
            presenter.device.raw().queue_submit(
                presenter.graphics_queue,
                &[submit_info],
                presenter.frame_finished,
            )?;
            let swapchains = [presenter.swapchain.as_ref().unwrap().handle];
            let image_indices = [self.image_index];
            let mut present_info = vk::PresentInfoKHR::default()
                .wait_semaphores(&signal_semaphores)
                .swapchains(&swapchains)
                .image_indices(&image_indices);
            let present_fences = if presenter.swapchain_maintenance1 {
                let release_fence = presenter
                    .swapchain
                    .as_ref()
                    .unwrap()
                    .presentation_release_fences[image_index];
                presenter.device.raw().reset_fences(&[release_fence])?;
                [release_fence]
            } else {
                [vk::Fence::null()]
            };
            let mut present_fence_info =
                vk::SwapchainPresentFenceInfoEXT::default().fences(&present_fences);
            if presenter.swapchain_maintenance1 {
                present_info = present_info.push_next(&mut present_fence_info);
            }
            let present_result = presenter
                .swapchain_loader
                .queue_present(presenter.present_queue, &present_info);
            if matches!(
                present_result,
                Ok(_) | Err(vk::Result::ERROR_OUT_OF_DATE_KHR | vk::Result::ERROR_SURFACE_LOST_KHR)
            ) {
                // Even a presentation rejected as out of date or surface lost
                // has enqueued its queue operations, so its release fence must
                // participate in the Swapchain's lifetime.
                presenter
                    .swapchain
                    .as_mut()
                    .unwrap()
                    .submitted_for_presentation[image_index] = true;
            }
            let present_suboptimal = match present_result {
                Ok(suboptimal) => suboptimal,
                Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => true,
                Err(error) => return Err(error.into()),
            };
            if self.acquire_suboptimal || present_suboptimal {
                presenter.swapchain_needs_recreation = true;
                return Ok(PresentOutcome::SwapchainInvalidated);
            }
        }
        Ok(PresentOutcome::Presented)
    }
}

fn timeout_nanoseconds(timeout: Duration) -> u64 {
    timeout.as_nanos().min(u128::from(u64::MAX - 1)) as u64
}

impl Drop for Presenter {
    fn drop(&mut self) {
        unsafe {
            let _ = self.device.raw().device_wait_idle();
            if self.swapchain_maintenance1 {
                let fences = self
                    .swapchain
                    .iter()
                    .chain(&self.retired_swapchain)
                    .flat_map(|resources| {
                        resources
                            .submitted_for_presentation
                            .iter()
                            .zip(&resources.presentation_release_fences)
                            .filter_map(|(&submitted, &fence)| submitted.then_some(fence))
                    })
                    .collect::<Vec<_>>();
                if !fences.is_empty() {
                    let _ = self.device.raw().wait_for_fences(&fences, true, u64::MAX);
                }
            }
            if let Some(resources) = self.swapchain.take() {
                destroy_swapchain_resources(
                    self.device.raw(),
                    &self.swapchain_loader,
                    self.command_pool,
                    resources,
                );
            }
            if let Some(resources) = self.retired_swapchain.take() {
                destroy_swapchain_resources(
                    self.device.raw(),
                    &self.swapchain_loader,
                    self.command_pool,
                    resources,
                );
            }
            self.device.raw().destroy_fence(self.frame_finished, None);
            self.device
                .raw()
                .destroy_semaphore(self.image_available, None);
            self.device
                .raw()
                .destroy_command_pool(self.command_pool, None);
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

#[allow(clippy::too_many_arguments)]
unsafe fn transition_image(
    device: &ash::Device,
    command_buffer: vk::CommandBuffer,
    image: vk::Image,
    old_layout: vk::ImageLayout,
    new_layout: vk::ImageLayout,
    src_stage_mask: vk::PipelineStageFlags2,
    src_access_mask: vk::AccessFlags2,
    dst_stage_mask: vk::PipelineStageFlags2,
    dst_access_mask: vk::AccessFlags2,
) {
    let barrier = vk::ImageMemoryBarrier2::default()
        .src_stage_mask(src_stage_mask)
        .src_access_mask(src_access_mask)
        .dst_stage_mask(dst_stage_mask)
        .dst_access_mask(dst_access_mask)
        .old_layout(old_layout)
        .new_layout(new_layout)
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .image(image)
        .subresource_range(
            vk::ImageSubresourceRange::default()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .level_count(1)
                .layer_count(1),
        );
    let dependency =
        vk::DependencyInfo::default().image_memory_barriers(std::slice::from_ref(&barrier));
    unsafe { device.cmd_pipeline_barrier2(command_buffer, &dependency) };
}

#[allow(clippy::too_many_arguments)]
unsafe fn create_swapchain_resources(
    surface_loader: &khr::surface::Instance,
    surface: vk::SurfaceKHR,
    physical_device: vk::PhysicalDevice,
    device: &ash::Device,
    swapchain_loader: &khr::swapchain::Device,
    command_pool: vk::CommandPool,
    queue_family_index: u32,
    requested_size: SurfaceSize,
    old_swapchain: vk::SwapchainKHR,
    swapchain_maintenance1: bool,
) -> Result<SwapchainResources> {
    let capabilities = unsafe {
        surface_loader.get_physical_device_surface_capabilities(physical_device, surface)?
    };
    let formats =
        unsafe { surface_loader.get_physical_device_surface_formats(physical_device, surface)? };
    let surface_format = formats
        .iter()
        .copied()
        .find(|format| {
            format.format == vk::Format::B8G8R8A8_SRGB
                && format.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR
        })
        .unwrap_or(formats[0]);
    let extent = if capabilities.current_extent.width != u32::MAX {
        capabilities.current_extent
    } else {
        vk::Extent2D {
            width: requested_size.width.clamp(
                capabilities.min_image_extent.width,
                capabilities.max_image_extent.width,
            ),
            height: requested_size.height.clamp(
                capabilities.min_image_extent.height,
                capabilities.max_image_extent.height,
            ),
        }
    };
    let mut image_count = capabilities.min_image_count + 1;
    if capabilities.max_image_count > 0 {
        image_count = image_count.min(capabilities.max_image_count);
    }

    let queue_family_indices = [queue_family_index];
    let swapchain_info = vk::SwapchainCreateInfoKHR::default()
        .surface(surface)
        .min_image_count(image_count)
        .image_format(surface_format.format)
        .image_color_space(surface_format.color_space)
        .image_extent(extent)
        .image_array_layers(1)
        .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
        .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
        .queue_family_indices(&queue_family_indices)
        .pre_transform(capabilities.current_transform)
        .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
        .present_mode(vk::PresentModeKHR::FIFO)
        .clipped(true)
        .old_swapchain(old_swapchain);
    let handle = unsafe { swapchain_loader.create_swapchain(&swapchain_info, None)? };
    let images = unsafe { swapchain_loader.get_swapchain_images(handle)? };

    let image_views = images
        .iter()
        .map(|&image| {
            let subresource = vk::ImageSubresourceRange::default()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .level_count(1)
                .layer_count(1);
            let view_info = vk::ImageViewCreateInfo::default()
                .image(image)
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(surface_format.format)
                .subresource_range(subresource);
            unsafe { device.create_image_view(&view_info, None) }
        })
        .collect::<std::result::Result<Vec<_>, _>>()?;

    let render_targets = images
        .iter()
        .copied()
        .zip(image_views.iter().copied())
        .map(|(image, image_view)| SurfaceRenderTarget {
            image,
            image_view,
            format: surface_format.format,
            extent,
        })
        .collect::<Vec<_>>();

    let allocation_info = vk::CommandBufferAllocateInfo::default()
        .command_pool(command_pool)
        .level(vk::CommandBufferLevel::PRIMARY)
        .command_buffer_count(render_targets.len() as u32);
    let command_buffers = unsafe { device.allocate_command_buffers(&allocation_info)? };
    let semaphore_info = vk::SemaphoreCreateInfo::default();
    let render_finished = (0..render_targets.len())
        .map(|_| unsafe { device.create_semaphore(&semaphore_info, None) })
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let presentation_release_fences = if swapchain_maintenance1 {
        let fence_info = vk::FenceCreateInfo::default();
        (0..render_targets.len())
            .map(|_| unsafe { device.create_fence(&fence_info, None) })
            .collect::<std::result::Result<Vec<_>, _>>()?
    } else {
        Vec::new()
    };
    let submitted_for_presentation = vec![false; render_targets.len()];

    Ok(SwapchainResources {
        handle,
        image_views,
        render_targets,
        command_buffers,
        render_finished,
        presentation_release_fences,
        submitted_for_presentation,
    })
}

unsafe fn presentation_resources_released(
    device: &ash::Device,
    resources: &SwapchainResources,
) -> std::result::Result<bool, vk::Result> {
    for (&submitted, &fence) in resources
        .submitted_for_presentation
        .iter()
        .zip(&resources.presentation_release_fences)
    {
        if submitted && !unsafe { device.get_fence_status(fence)? } {
            return Ok(false);
        }
    }
    Ok(true)
}

unsafe fn destroy_swapchain_resources(
    device: &ash::Device,
    swapchain_loader: &khr::swapchain::Device,
    command_pool: vk::CommandPool,
    resources: SwapchainResources,
) {
    unsafe {
        device.free_command_buffers(command_pool, &resources.command_buffers);
        for fence in resources.presentation_release_fences {
            device.destroy_fence(fence, None);
        }
        for semaphore in resources.render_finished {
            device.destroy_semaphore(semaphore, None);
        }
        for image_view in resources.image_views {
            device.destroy_image_view(image_view, None);
        }
        swapchain_loader.destroy_swapchain(resources.handle, None);
    }
}
