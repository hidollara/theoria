//! Resources and image transitions owned by one Swapchain generation.

use std::error::Error;

use ash::vk;

use crate::surface::SurfaceSize;

use super::device::PresentationDevice;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

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
        let images = unsafe { device.swapchain_loader.get_swapchain_images(handle)? };

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
                unsafe { device.render.raw().create_image_view(&view_info, None) }
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
        let command_buffers = unsafe {
            device
                .render
                .raw()
                .allocate_command_buffers(&allocation_info)?
        };
        let semaphore_info = vk::SemaphoreCreateInfo::default();
        let render_finished = (0..render_targets.len())
            .map(|_| unsafe { device.render.raw().create_semaphore(&semaphore_info, None) })
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let presentation_release_fences = if device.swapchain_maintenance1 {
            let fence_info = vk::FenceCreateInfo::default();
            (0..render_targets.len())
                .map(|_| unsafe { device.render.raw().create_fence(&fence_info, None) })
                .collect::<std::result::Result<Vec<_>, _>>()?
        } else {
            Vec::new()
        };
        let submitted_for_presentation = vec![false; render_targets.len()];

        Ok(Self {
            handle,
            image_views,
            render_targets,
            command_buffers,
            render_finished,
            presentation_release_fences,
            submitted_for_presentation,
        })
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
            device
                .render
                .raw()
                .free_command_buffers(command_pool, &self.command_buffers);
            for fence in self.presentation_release_fences {
                device.render.raw().destroy_fence(fence, None);
            }
            for semaphore in self.render_finished {
                device.render.raw().destroy_semaphore(semaphore, None);
            }
            for image_view in self.image_views {
                device.render.raw().destroy_image_view(image_view, None);
            }
            device.swapchain_loader.destroy_swapchain(self.handle, None);
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

#[allow(clippy::too_many_arguments)]
pub(super) unsafe fn transition_image(
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
