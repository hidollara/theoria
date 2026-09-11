use std::{
    error::Error,
    time::{Duration, Instant},
};

use ash::vk;

use crate::{color::Color, surface::SurfaceSize, target::PresentationTarget};

mod device;
mod swapchain;

use device::PresentationDevice;
pub use device::RenderDevice;
use swapchain::{SurfaceRenderTarget, SwapchainResources, transition_image};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

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
    device: PresentationDevice,
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
            let device = PresentationDevice::new(target)?;

            let command_pool_info = vk::CommandPoolCreateInfo::default()
                .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
                .queue_family_index(device.render.queue_family_index());
            let command_pool = device
                .render
                .raw()
                .create_command_pool(&command_pool_info, None)?;
            let semaphore_info = vk::SemaphoreCreateInfo::default();
            let image_available = device
                .render
                .raw()
                .create_semaphore(&semaphore_info, None)?;
            let fence_info = vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED);
            let frame_finished = device.render.raw().create_fence(&fence_info, None)?;

            let mut renderer = Self {
                device,
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
        self.device.render.clone()
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
            match self.device.render.raw().wait_for_fences(
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
            let (image_index, acquire_suboptimal) =
                match self.device.swapchain_loader.acquire_next_image(
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
                .render
                .raw()
                .reset_command_buffer(command_buffer, vk::CommandBufferResetFlags::empty())?;
            self.device
                .render
                .raw()
                .begin_command_buffer(command_buffer, &vk::CommandBufferBeginInfo::default())?;
            transition_image(
                self.device.render.raw(),
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
            if self.swapchain.is_some() && !self.device.swapchain_maintenance1 {
                // Without presentation fences, Vulkan provides no portable way
                // to prove that a retired Swapchain is no longer in use.
                self.device.render.raw().device_wait_idle()?;
            }

            let old_swapchain = self
                .swapchain
                .as_ref()
                .map_or(vk::SwapchainKHR::null(), |resources| resources.handle);
            let new_swapchain = SwapchainResources::new(
                &self.device,
                self.command_pool,
                self.requested_size,
                old_swapchain,
            )?;

            if let Some(old) = self.swapchain.replace(new_swapchain) {
                if self.device.swapchain_maintenance1 {
                    debug_assert!(self.retired_swapchain.is_none());
                    self.retired_swapchain = Some(old);
                } else {
                    old.destroy(&self.device, self.command_pool);
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
                if submitted && !unsafe { self.device.render.raw().get_fence_status(fence)? } {
                    unsafe {
                        match self.device.render.raw().wait_for_fences(
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
            unsafe { resources.released(self.device.render.raw())? }
        } else {
            false
        };
        if ready {
            let resources = self
                .retired_swapchain
                .take()
                .expect("a ready retired Swapchain is present");
            unsafe {
                resources.destroy(&self.device, self.command_pool);
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
                presenter.device.render.raw(),
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
                .render
                .raw()
                .end_command_buffer(self.command_buffer)?;
            presenter
                .device
                .render
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
            presenter.device.render.raw().queue_submit(
                presenter.device.render.graphics_queue(),
                &[submit_info],
                presenter.frame_finished,
            )?;
            let swapchains = [presenter.swapchain.as_ref().unwrap().handle];
            let image_indices = [self.image_index];
            let mut present_info = vk::PresentInfoKHR::default()
                .wait_semaphores(&signal_semaphores)
                .swapchains(&swapchains)
                .image_indices(&image_indices);
            let present_fences = if presenter.device.swapchain_maintenance1 {
                let release_fence = presenter
                    .swapchain
                    .as_ref()
                    .unwrap()
                    .presentation_release_fences[image_index];
                presenter
                    .device
                    .render
                    .raw()
                    .reset_fences(&[release_fence])?;
                [release_fence]
            } else {
                [vk::Fence::null()]
            };
            let mut present_fence_info =
                vk::SwapchainPresentFenceInfoEXT::default().fences(&present_fences);
            if presenter.device.swapchain_maintenance1 {
                present_info = present_info.push_next(&mut present_fence_info);
            }
            let present_result = presenter
                .device
                .swapchain_loader
                .queue_present(presenter.device.present_queue, &present_info);
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
            let _ = self.device.render.raw().device_wait_idle();
            if self.device.swapchain_maintenance1 {
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
                    let _ = self
                        .device
                        .render
                        .raw()
                        .wait_for_fences(&fences, true, u64::MAX);
                }
            }
            if let Some(resources) = self.swapchain.take() {
                resources.destroy(&self.device, self.command_pool);
            }
            if let Some(resources) = self.retired_swapchain.take() {
                resources.destroy(&self.device, self.command_pool);
            }
            self.device
                .render
                .raw()
                .destroy_fence(self.frame_finished, None);
            self.device
                .render
                .raw()
                .destroy_semaphore(self.image_available, None);
            self.device
                .render
                .raw()
                .destroy_command_pool(self.command_pool, None);
        }
    }
}
