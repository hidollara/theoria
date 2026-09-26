//! Upload of CPU RGBA pixels into device-local Vulkan images.

use std::{error::Error, sync::Arc};

use ash::vk;

use crate::{
    image::RasterPlane,
    vulkan::{
        host_buffer::HostBuffer, image_layout::transition_image_layers, one_shot::OneShotCommands,
        presenter::RenderDevice,
    },
};

use super::storage::VulkanImage;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

pub(super) fn upload_rgba8(
    device: RenderDevice,
    width: u32,
    height: u32,
    plane: &RasterPlane<u8>,
) -> Result<Arc<VulkanImage>> {
    upload_rgba(
        device,
        width,
        height,
        plane.samples(),
        plane.row_stride(),
        4,
        vk::Format::R8G8B8A8_UNORM,
        1,
        false,
    )
}

pub(super) fn upload_rgba8_array(
    device: RenderDevice,
    width: u32,
    height: u32,
    samples: &[u8],
    layers: u32,
) -> Result<Arc<VulkanImage>> {
    upload_rgba(
        device,
        width,
        height,
        samples,
        usize::try_from(width)? * 4,
        4,
        vk::Format::R8G8B8A8_UNORM,
        layers,
        true,
    )
}

fn upload_rgba(
    device: RenderDevice,
    width: u32,
    height: u32,
    pixels: &[u8],
    row_stride: usize,
    bytes_per_pixel: usize,
    format: vk::Format,
    layers: u32,
    array_view: bool,
) -> Result<Arc<VulkanImage>> {
    let staging = HostBuffer::new(
        device.clone(),
        pixels.len() as vk::DeviceSize,
        vk::BufferUsageFlags::TRANSFER_SRC,
    )?;
    staging.write(pixels)?;

    let extent = vk::Extent2D { width, height };
    let backing = Arc::new(VulkanImage::new_with_layers(
        device.clone(),
        // Preserve stored coordinates exactly. Transfer decoding happens in
        // the image shader after straight/premultiplied alpha is interpreted.
        format,
        extent,
        vk::SampleCountFlags::TYPE_1,
        vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED,
        layers,
        array_view,
    )?);

    let commands = OneShotCommands::begin(device.clone())?;
    let command_buffer = commands.buffer();
    unsafe {
        transition_image_layers(
            device.raw(),
            command_buffer,
            backing.image,
            vk::ImageLayout::UNDEFINED,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            vk::PipelineStageFlags2::NONE,
            vk::AccessFlags2::NONE,
            vk::PipelineStageFlags2::COPY,
            vk::AccessFlags2::TRANSFER_WRITE,
            layers,
        );
        let region = vk::BufferImageCopy::default()
            .buffer_row_length(u32::try_from(row_stride / bytes_per_pixel)?)
            .image_subresource(
                vk::ImageSubresourceLayers::default()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .layer_count(layers),
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
        transition_image_layers(
            device.raw(),
            command_buffer,
            backing.image,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            vk::PipelineStageFlags2::COPY,
            vk::AccessFlags2::TRANSFER_WRITE,
            vk::PipelineStageFlags2::FRAGMENT_SHADER,
            vk::AccessFlags2::SHADER_SAMPLED_READ,
            layers,
        );
    }
    commands.submit_and_wait()?;
    Ok(backing)
}
