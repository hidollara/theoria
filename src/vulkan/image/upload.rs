//! Upload of CPU RGBA8 pixels into device-local Vulkan images.

use std::{error::Error, sync::Arc};

use ash::vk;

use crate::{
    image::Image,
    vulkan::{
        host_buffer::HostBuffer, image_layout::transition_image, one_shot::OneShotCommands,
        presenter::RenderDevice,
    },
};

use super::storage::VulkanImage;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

pub(super) fn upload_rgba8(device: RenderDevice, source: &Image) -> Result<Arc<VulkanImage>> {
    let pixels = source.rgba8();
    let staging = HostBuffer::new(
        device.clone(),
        pixels.len() as vk::DeviceSize,
        vk::BufferUsageFlags::TRANSFER_SRC,
    )?;
    staging.write(pixels)?;

    let extent = vk::Extent2D {
        width: source.width(),
        height: source.height(),
    };
    let backing = Arc::new(VulkanImage::new(
        device.clone(),
        vk::Format::R8G8B8A8_SRGB,
        extent,
        vk::SampleCountFlags::TYPE_1,
        vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED,
    )?);

    let commands = OneShotCommands::begin(device.clone())?;
    let command_buffer = commands.buffer();
    unsafe {
        transition_image(
            device.raw(),
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
        device.raw().cmd_copy_buffer_to_image(
            command_buffer,
            staging.raw(),
            backing.image,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            &[region],
        );
        transition_image(
            device.raw(),
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
