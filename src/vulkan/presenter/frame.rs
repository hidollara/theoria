//! An acquired presentation image and the operations valid before it is presented.

use std::{error::Error, marker::PhantomData};

use ash::vk;

use crate::{
    canvas::Color,
    color::{IccProfile, IccRenderingIntent, LinearSrgb, Srgb},
    surface::{OutputAlpha, OutputImage},
    vulkan::image_layout::transition_image,
};

use super::{device::PresentationDevice, swapchain::SwapchainResources};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

pub(super) struct FrameSynchronization {
    pub(super) image_available: vk::Semaphore,
    pub(super) finished: vk::Fence,
}

impl FrameSynchronization {
    pub(super) unsafe fn new(device: &ash::Device) -> Result<Self> {
        let image_available =
            unsafe { device.create_semaphore(&vk::SemaphoreCreateInfo::default(), None)? };
        let fence_info = vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED);
        let finished = match unsafe { device.create_fence(&fence_info, None) } {
            Ok(fence) => fence,
            Err(error) => {
                unsafe { device.destroy_semaphore(image_available, None) };
                return Err(error.into());
            }
        };
        Ok(Self {
            image_available,
            finished,
        })
    }

    pub(super) unsafe fn destroy(&self, device: &ash::Device) {
        unsafe {
            device.destroy_fence(self.finished, None);
            device.destroy_semaphore(self.image_available, None);
        }
    }
}

/// An acquired destination that a renderer can record commands into.
pub struct RenderTarget<'a> {
    pub(crate) command_buffer: vk::CommandBuffer,
    image_view: vk::ImageView,
    format: vk::Format,
    extent: vk::Extent2D,
    clear_color: Color,
    output: OutputImage,
    _frame: PhantomData<&'a ()>,
}

impl<'a> RenderTarget<'a> {
    pub(in crate::vulkan) fn new(
        command_buffer: vk::CommandBuffer,
        image_view: vk::ImageView,
        format: vk::Format,
        extent: vk::Extent2D,
        clear_color: Color,
    ) -> Self {
        Self {
            command_buffer,
            image_view,
            format,
            extent,
            clear_color,
            output: OutputImage::new(LinearSrgb, OutputAlpha::Premultiplied),
            _frame: PhantomData,
        }
    }

    pub(crate) fn surface(&self) -> (vk::ImageView, vk::Format, vk::Extent2D) {
        (self.image_view, self.format, self.extent)
    }

    pub(crate) fn clear_color(&self) -> Color {
        self.clear_color.clone()
    }

    pub(crate) fn output(&self) -> &OutputImage {
        &self.output
    }

    /// Selects the color and alpha meaning of the destination pixels.
    /// The Vulkan attachment format must be compatible with this description.
    pub fn with_output(mut self, output: OutputImage) -> Self {
        self.output = output;
        self
    }

    /// Encodes the final working image into ICC device RGB coordinates.
    /// The target format must not perform an additional sRGB encoding.
    pub fn with_icc_output(mut self, profile: &IccProfile, intent: IccRenderingIntent) -> Self {
        self.output = OutputImage::new(profile.color_space(intent), OutputAlpha::Premultiplied);
        self
    }
}

/// One acquired Swapchain image and the synchronization needed to present it.
pub struct PresentFrame<'a> {
    device: &'a PresentationDevice,
    swapchain: &'a mut SwapchainResources,
    synchronization: &'a FrameSynchronization,
    swapchain_needs_recreation: &'a mut bool,
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

impl<'a> PresentFrame<'a> {
    pub(super) fn new(
        device: &'a PresentationDevice,
        swapchain: &'a mut SwapchainResources,
        synchronization: &'a FrameSynchronization,
        swapchain_needs_recreation: &'a mut bool,
        image_index: u32,
        command_buffer: vk::CommandBuffer,
        acquire_suboptimal: bool,
    ) -> Self {
        Self {
            device,
            swapchain,
            synchronization,
            swapchain_needs_recreation,
            image_index,
            command_buffer,
            acquire_suboptimal,
        }
    }

    pub fn render_target(&self, clear_color: impl Into<Color>) -> RenderTarget<'_> {
        let surface = self.swapchain.render_targets[self.image_index as usize];
        RenderTarget::new(
            self.command_buffer,
            surface.image_view,
            surface.format,
            surface.extent,
            clear_color.into(),
        )
        .with_output(OutputImage::new(Srgb, OutputAlpha::Opaque))
    }

    /// Submits the recorded frame and presents it.
    ///
    /// [`PresentOutcome::SwapchainInvalidated`] means that the submitted image
    /// belonged to a Swapchain which must be replaced before the next frame.
    pub fn present(self) -> Result<PresentOutcome> {
        unsafe {
            let Self {
                device,
                swapchain,
                synchronization,
                swapchain_needs_recreation,
                image_index,
                command_buffer,
                acquire_suboptimal,
            } = self;
            let image_slot = image_index as usize;
            let image = swapchain.render_targets[image_slot].image;
            let render_finished = swapchain.render_finished[image_slot];
            transition_image(
                device.render.raw(),
                command_buffer,
                image,
                vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                vk::ImageLayout::PRESENT_SRC_KHR,
                vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
                vk::PipelineStageFlags2::NONE,
                vk::AccessFlags2::NONE,
            );
            device.render.raw().end_command_buffer(command_buffer)?;
            device
                .render
                .raw()
                .reset_fences(&[synchronization.finished])?;
            let wait_semaphores = [synchronization.image_available];
            let signal_semaphores = [render_finished];
            let wait_stages = [vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
            let command_buffers = [command_buffer];
            let submit_info = vk::SubmitInfo::default()
                .wait_semaphores(&wait_semaphores)
                .wait_dst_stage_mask(&wait_stages)
                .command_buffers(&command_buffers)
                .signal_semaphores(&signal_semaphores);
            device.render.raw().queue_submit(
                device.render.graphics_queue(),
                &[submit_info],
                synchronization.finished,
            )?;
            let swapchains = [swapchain.handle];
            let image_indices = [image_index];
            let mut present_info = vk::PresentInfoKHR::default()
                .wait_semaphores(&signal_semaphores)
                .swapchains(&swapchains)
                .image_indices(&image_indices);
            let present_fences = if device.swapchain_maintenance1 {
                let release_fence = swapchain.presentation_release_fences[image_slot];
                device.render.raw().reset_fences(&[release_fence])?;
                [release_fence]
            } else {
                [vk::Fence::null()]
            };
            let mut present_fence_info =
                vk::SwapchainPresentFenceInfoEXT::default().fences(&present_fences);
            if device.swapchain_maintenance1 {
                present_info = present_info.push_next(&mut present_fence_info);
            }
            let present_result = device
                .swapchain_loader
                .queue_present(device.present_queue, &present_info);
            if matches!(
                present_result,
                Ok(_) | Err(vk::Result::ERROR_OUT_OF_DATE_KHR | vk::Result::ERROR_SURFACE_LOST_KHR)
            ) {
                // Even a presentation rejected as out of date or surface lost
                // has enqueued its queue operations, so its release fence must
                // participate in the Swapchain's lifetime.
                swapchain.submitted_for_presentation[image_slot] = true;
            }
            let present_suboptimal = match present_result {
                Ok(suboptimal) => suboptimal,
                Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => true,
                Err(error) => return Err(error.into()),
            };
            if acquire_suboptimal || present_suboptimal {
                *swapchain_needs_recreation = true;
                return Ok(PresentOutcome::SwapchainInvalidated);
            }
        }
        Ok(PresentOutcome::Presented)
    }
}
