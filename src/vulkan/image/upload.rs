//! Upload of CPU RGBA8 pixels into device-local Vulkan images.

use std::{error::Error, ptr, sync::Arc};

use ash::vk;

use crate::{
    image::Image,
    vulkan::{image_layout::transition_image, one_shot::OneShotCommands, presenter::RenderDevice},
};

use super::storage::VulkanImage;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

struct StagingBuffer {
    device: RenderDevice,
    buffer: vk::Buffer,
    memory: vk::DeviceMemory,
}

impl StagingBuffer {
    fn new(device: RenderDevice, size: vk::DeviceSize) -> Result<Self> {
        let raw = device.raw();
        let buffer = unsafe {
            raw.create_buffer(
                &vk::BufferCreateInfo::default()
                    .size(size)
                    .usage(vk::BufferUsageFlags::TRANSFER_SRC)
                    .sharing_mode(vk::SharingMode::EXCLUSIVE),
                None,
            )?
        };
        let requirements = unsafe { raw.get_buffer_memory_requirements(buffer) };
        let Some(memory_type) = device.find_memory_type(
            requirements.memory_type_bits,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        ) else {
            unsafe { raw.destroy_buffer(buffer, None) };
            return Err("no host-visible coherent Vulkan memory type for image upload".into());
        };
        let memory = match unsafe {
            raw.allocate_memory(
                &vk::MemoryAllocateInfo::default()
                    .allocation_size(requirements.size)
                    .memory_type_index(memory_type),
                None,
            )
        } {
            Ok(memory) => memory,
            Err(error) => {
                unsafe { raw.destroy_buffer(buffer, None) };
                return Err(error.into());
            }
        };
        if let Err(error) = unsafe { raw.bind_buffer_memory(buffer, memory, 0) } {
            unsafe {
                raw.free_memory(memory, None);
                raw.destroy_buffer(buffer, None);
            }
            return Err(error.into());
        }
        Ok(Self {
            device,
            buffer,
            memory,
        })
    }

    fn write(&self, bytes: &[u8]) -> Result<()> {
        let mapped = unsafe {
            self.device.raw().map_memory(
                self.memory,
                0,
                bytes.len() as vk::DeviceSize,
                vk::MemoryMapFlags::empty(),
            )?
        };
        unsafe {
            ptr::copy_nonoverlapping(bytes.as_ptr(), mapped.cast(), bytes.len());
            self.device.raw().unmap_memory(self.memory);
        }
        Ok(())
    }
}

impl Drop for StagingBuffer {
    fn drop(&mut self) {
        unsafe {
            self.device.raw().destroy_buffer(self.buffer, None);
            self.device.raw().free_memory(self.memory, None);
        }
    }
}

pub(super) fn upload_rgba8(device: RenderDevice, source: &Image) -> Result<Arc<VulkanImage>> {
    let pixels = source
        .rgba8()
        .ok_or("the Image has neither RGBA8 pixels nor a compatible Vulkan materialization")?;
    let staging = StagingBuffer::new(device.clone(), pixels.len() as vk::DeviceSize)?;
    staging.write(pixels)?;

    let extent = vk::Extent2D {
        width: source.width(),
        height: source.height(),
    };
    let backing = Arc::new(VulkanImage::new(
        device.clone(),
        vk::Format::R8G8B8A8_SRGB,
        extent,
        vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED,
    )?);

    let commands = OneShotCommands::begin(device)?;
    let command_buffer = commands.buffer();
    unsafe {
        transition_image(
            staging.device.raw(),
            command_buffer,
            backing.image,
            vk::ImageLayout::UNDEFINED,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            vk::PipelineStageFlags2::NONE,
            vk::AccessFlags2::NONE,
            vk::PipelineStageFlags2::COPY,
            vk::AccessFlags2::TRANSFER_WRITE,
        );
        let region = vk::BufferImageCopy::default()
            .image_subresource(
                vk::ImageSubresourceLayers::default()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .layer_count(1),
            )
            .image_extent(vk::Extent3D {
                width: extent.width,
                height: extent.height,
                depth: 1,
            });
        staging.device.raw().cmd_copy_buffer_to_image(
            command_buffer,
            staging.buffer,
            backing.image,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            &[region],
        );
        transition_image(
            staging.device.raw(),
            command_buffer,
            backing.image,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            vk::PipelineStageFlags2::COPY,
            vk::AccessFlags2::TRANSFER_WRITE,
            vk::PipelineStageFlags2::FRAGMENT_SHADER,
            vk::AccessFlags2::SHADER_SAMPLED_READ,
        );
    }
    commands.submit_and_wait()?;
    Ok(backing)
}
