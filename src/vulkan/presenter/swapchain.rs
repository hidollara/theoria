//! Resources owned by one Swapchain generation.

use std::error::Error;

use ash::vk;

use crate::surface::SurfaceSize;

use super::device::PresentationDevice;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

struct SwapchainConstruction<'a> {
    device: &'a PresentationDevice,
    command_pool: vk::CommandPool,
    handle: vk::SwapchainKHR,
    image_views: Vec<vk::ImageView>,
    command_buffers: Vec<vk::CommandBuffer>,
    render_finished: Vec<vk::Semaphore>,
    presentation_release_fences: Vec<vk::Fence>,
}

impl<'a> SwapchainConstruction<'a> {
    fn new(
        device: &'a PresentationDevice,
        command_pool: vk::CommandPool,
        handle: vk::SwapchainKHR,
    ) -> Self {
        Self {
            device,
            command_pool,
            handle,
            image_views: Vec::new(),
            command_buffers: Vec::new(),
            render_finished: Vec::new(),
            presentation_release_fences: Vec::new(),
        }
    }

    fn finish(mut self, render_targets: Vec<SurfaceRenderTarget>) -> SwapchainResources {
        let submitted_for_presentation = vec![false; render_targets.len()];
        let resources = SwapchainResources {
            handle: self.handle,
            image_views: std::mem::take(&mut self.image_views),
            render_targets,
            command_buffers: std::mem::take(&mut self.command_buffers),
            render_finished: std::mem::take(&mut self.render_finished),
            presentation_release_fences: std::mem::take(&mut self.presentation_release_fences),
            submitted_for_presentation,
        };
        self.handle = vk::SwapchainKHR::null();
        resources
    }
}

impl Drop for SwapchainConstruction<'_> {
    fn drop(&mut self) {
        unsafe {
            destroy_swapchain_parts(
                self.device,
                self.command_pool,
                self.handle,
                &self.command_buffers,
                &self.presentation_release_fences,
                &self.render_finished,
                &self.image_views,
            );
        }
    }
}

pub(super) struct SwapchainResources {
    pub(super) handle: vk::SwapchainKHR,
    pub(super) image_views: Vec<vk::ImageView>,
    pub(super) render_targets: Vec<SurfaceRenderTarget>,
    pub(super) command_buffers: Vec<vk::CommandBuffer>,
    pub(super) render_finished: Vec<vk::Semaphore>,
    pub(super) presentation_release_fences: Vec<vk::Fence>,
    pub(super) submitted_for_presentation: Vec<bool>,
}

impl SwapchainResources {
    pub(super) unsafe fn new(
        device: &PresentationDevice,
        command_pool: vk::CommandPool,
        requested_size: SurfaceSize,
        old_swapchain: vk::SwapchainKHR,
    ) -> Result<Self> {
        let capabilities = unsafe {
            device
                .surface_loader
                .get_physical_device_surface_capabilities(device.physical_device, device.surface)?
        };
        let formats = unsafe {
            device
                .surface_loader
                .get_physical_device_surface_formats(device.physical_device, device.surface)?
        };
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

        let queue_family_indices = [device.render.queue_family_index()];
        let swapchain_info = vk::SwapchainCreateInfoKHR::default()
            .surface(device.surface)
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
        let handle = unsafe {
            device
                .swapchain_loader
                .create_swapchain(&swapchain_info, None)?
        };
        let mut construction = SwapchainConstruction::new(device, command_pool, handle);
        let images = unsafe { device.swapchain_loader.get_swapchain_images(handle)? };

        for &image in &images {
            let subresource = vk::ImageSubresourceRange::default()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .level_count(1)
                .layer_count(1);
            let view_info = vk::ImageViewCreateInfo::default()
                .image(image)
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(surface_format.format)
                .subresource_range(subresource);
            construction
                .image_views
                .push(unsafe { device.render.raw().create_image_view(&view_info, None)? });
        }

        let render_targets = images
            .iter()
            .copied()
            .zip(construction.image_views.iter().copied())
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
        construction.command_buffers = unsafe {
            device
                .render
                .raw()
                .allocate_command_buffers(&allocation_info)?
        };
        let semaphore_info = vk::SemaphoreCreateInfo::default();
        for _ in 0..render_targets.len() {
            construction.render_finished.push(unsafe {
                device
                    .render
                    .raw()
                    .create_semaphore(&semaphore_info, None)?
            });
        }
        if device.swapchain_maintenance1 {
            let fence_info = vk::FenceCreateInfo::default();
            for _ in 0..render_targets.len() {
                construction
                    .presentation_release_fences
                    .push(unsafe { device.render.raw().create_fence(&fence_info, None)? });
            }
        }

        Ok(construction.finish(render_targets))
    }

    pub(super) unsafe fn released(
        &self,
        device: &ash::Device,
    ) -> std::result::Result<bool, vk::Result> {
        for (&submitted, &fence) in self
            .submitted_for_presentation
            .iter()
            .zip(&self.presentation_release_fences)
        {
            if submitted && !unsafe { device.get_fence_status(fence)? } {
                return Ok(false);
            }
        }
        Ok(true)
    }

    pub(super) unsafe fn destroy(self, device: &PresentationDevice, command_pool: vk::CommandPool) {
        unsafe {
            destroy_swapchain_parts(
                device,
                command_pool,
                self.handle,
                &self.command_buffers,
                &self.presentation_release_fences,
                &self.render_finished,
                &self.image_views,
            );
        }
    }
}

unsafe fn destroy_swapchain_parts(
    device: &PresentationDevice,
    command_pool: vk::CommandPool,
    handle: vk::SwapchainKHR,
    command_buffers: &[vk::CommandBuffer],
    presentation_release_fences: &[vk::Fence],
    render_finished: &[vk::Semaphore],
    image_views: &[vk::ImageView],
) {
    unsafe {
        if !command_buffers.is_empty() {
            device
                .render
                .raw()
                .free_command_buffers(command_pool, command_buffers);
        }
        for &fence in presentation_release_fences {
            device.render.raw().destroy_fence(fence, None);
        }
        for &semaphore in render_finished {
            device.render.raw().destroy_semaphore(semaphore, None);
        }
        for &image_view in image_views {
            device.render.raw().destroy_image_view(image_view, None);
        }
        if handle != vk::SwapchainKHR::null() {
            device.swapchain_loader.destroy_swapchain(handle, None);
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct SurfaceRenderTarget {
    pub(super) image: vk::Image,
    pub(super) image_view: vk::ImageView,
    pub(super) format: vk::Format,
    pub(super) extent: vk::Extent2D,
}
