//! One-shot Vulkan command recording and synchronous submission.

use std::error::Error;

use ash::vk;

use crate::vulkan::presenter::RenderDevice;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

/// Owns one transient primary command buffer and its pool.
pub(super) struct OneShotCommands {
    device: RenderDevice,
    pool: vk::CommandPool,
    buffer: vk::CommandBuffer,
}

impl OneShotCommands {
    pub(super) fn begin(device: RenderDevice) -> Result<Self> {
        let raw = device.raw();
        let pool = unsafe {
            raw.create_command_pool(
                &vk::CommandPoolCreateInfo::default()
                    .flags(vk::CommandPoolCreateFlags::TRANSIENT)
                    .queue_family_index(device.queue_family_index()),
                None,
            )?
        };
        let buffer = match unsafe {
            raw.allocate_command_buffers(
                &vk::CommandBufferAllocateInfo::default()
                    .command_pool(pool)
                    .level(vk::CommandBufferLevel::PRIMARY)
                    .command_buffer_count(1),
            )
        } {
            Ok(buffers) => buffers[0],
            Err(error) => {
                unsafe { raw.destroy_command_pool(pool, None) };
                return Err(error.into());
            }
        };
        if let Err(error) = unsafe {
            raw.begin_command_buffer(
                buffer,
                &vk::CommandBufferBeginInfo::default()
                    .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
            )
        } {
            unsafe { raw.destroy_command_pool(pool, None) };
            return Err(error.into());
        }
        Ok(Self {
            device,
            pool,
            buffer,
        })
    }

    pub(super) fn buffer(&self) -> vk::CommandBuffer {
        self.buffer
    }

    pub(super) fn submit_and_wait(self) -> Result<()> {
        let raw = self.device.raw();
        unsafe { raw.end_command_buffer(self.buffer)? };
        let fence = unsafe { raw.create_fence(&vk::FenceCreateInfo::default(), None)? };
        let buffers = [self.buffer];
        let submit = vk::SubmitInfo::default().command_buffers(&buffers);
        let result = unsafe {
            raw.queue_submit(self.device.graphics_queue(), &[submit], fence)
                .and_then(|()| raw.wait_for_fences(&[fence], true, u64::MAX))
        };
        unsafe { raw.destroy_fence(fence, None) };
        result?;
        Ok(())
    }
}

impl Drop for OneShotCommands {
    fn drop(&mut self) {
        unsafe { self.device.raw().destroy_command_pool(self.pool, None) };
    }
}
