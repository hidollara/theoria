use std::{collections::HashMap, error::Error, ptr};

use ash::vk;

use crate::{
    canvas::{ImageExtend, ImagePattern, ImageSampling},
    image::{Image, ImageInstanceKey},
};

use super::presenter::RenderDevice;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

pub(super) struct GpuImage {
    image: vk::Image,
    view: vk::ImageView,
    memory: vk::DeviceMemory,
    descriptor_pool: vk::DescriptorPool,
    descriptor_sets: HashMap<SamplerKey, vk::DescriptorSet>,
}

pub(super) struct ImageCache {
    device: RenderDevice,
    samplers: HashMap<SamplerKey, vk::Sampler>,
    descriptor_set_layout: vk::DescriptorSetLayout,
    images: HashMap<ImageInstanceKey, GpuImage>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct SamplerKey {
    sampling: ImageSampling,
    extend_x: ImageExtend,
    extend_y: ImageExtend,
}

impl SamplerKey {
    const RENDER_IMAGE: Self = Self {
        sampling: ImageSampling::Linear,
        extend_x: ImageExtend::Clamp,
        extend_y: ImageExtend::Clamp,
    };

    fn from_pattern(pattern: &ImagePattern) -> Self {
        Self {
            sampling: pattern.sampling(),
            extend_x: pattern.extend_x(),
            extend_y: pattern.extend_y(),
        }
    }
}

impl GpuImage {
    pub(super) fn descriptor_set(&self, pattern: &ImagePattern) -> vk::DescriptorSet {
        self.descriptor_sets[&SamplerKey::from_pattern(pattern)]
    }
}

const fn image_extend_modes() -> [ImageExtend; 4] {
    [
        ImageExtend::Transparent,
        ImageExtend::Clamp,
        ImageExtend::Repeat,
        ImageExtend::Mirror,
    ]
}

fn sampler_info(key: SamplerKey) -> vk::SamplerCreateInfo<'static> {
    let filter = match key.sampling {
        ImageSampling::Nearest => vk::Filter::NEAREST,
        ImageSampling::Linear => vk::Filter::LINEAR,
    };
    vk::SamplerCreateInfo::default()
        .mag_filter(filter)
        .min_filter(filter)
        .mipmap_mode(vk::SamplerMipmapMode::NEAREST)
        .address_mode_u(address_mode(key.extend_x))
        .address_mode_v(address_mode(key.extend_y))
        .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
        .border_color(vk::BorderColor::FLOAT_TRANSPARENT_BLACK)
        .min_lod(0.0)
        .max_lod(0.0)
}

const fn address_mode(extend: ImageExtend) -> vk::SamplerAddressMode {
    match extend {
        ImageExtend::Transparent => vk::SamplerAddressMode::CLAMP_TO_BORDER,
        ImageExtend::Clamp => vk::SamplerAddressMode::CLAMP_TO_EDGE,
        ImageExtend::Repeat => vk::SamplerAddressMode::REPEAT,
        ImageExtend::Mirror => vk::SamplerAddressMode::MIRRORED_REPEAT,
    }
}

pub(super) struct SampledImageBinding {
    device: RenderDevice,
    descriptor_pool: vk::DescriptorPool,
    pub(super) descriptor_set: vk::DescriptorSet,
}

impl Drop for SampledImageBinding {
    fn drop(&mut self) {
        unsafe {
            self.device
                .raw()
                .destroy_descriptor_pool(self.descriptor_pool, None);
        }
    }
}

impl ImageCache {
    pub(super) fn new(device: RenderDevice) -> Result<Self> {
        unsafe {
            let bindings = [
                vk::DescriptorSetLayoutBinding::default()
                    .binding(0)
                    .descriptor_type(vk::DescriptorType::SAMPLER)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::FRAGMENT),
                vk::DescriptorSetLayoutBinding::default()
                    .binding(1)
                    .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::FRAGMENT),
            ];
            let descriptor_set_layout = device.raw().create_descriptor_set_layout(
                &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
                None,
            )?;
            let mut samplers = HashMap::new();
            for sampling in [ImageSampling::Nearest, ImageSampling::Linear] {
                for extend_x in image_extend_modes() {
                    for extend_y in image_extend_modes() {
                        let key = SamplerKey {
                            sampling,
                            extend_x,
                            extend_y,
                        };
                        match device.raw().create_sampler(&sampler_info(key), None) {
                            Ok(sampler) => {
                                samplers.insert(key, sampler);
                            }
                            Err(error) => {
                                for sampler in samplers.into_values() {
                                    device.raw().destroy_sampler(sampler, None);
                                }
                                device
                                    .raw()
                                    .destroy_descriptor_set_layout(descriptor_set_layout, None);
                                return Err(error.into());
                            }
                        }
                    }
                }
            }
            Ok(Self {
                device,
                samplers,
                descriptor_set_layout,
                images: HashMap::new(),
            })
        }
    }

    pub(super) fn descriptor_set_layout(&self) -> vk::DescriptorSetLayout {
        self.descriptor_set_layout
    }

    pub(super) fn get_or_upload(&mut self, image: &Image) -> Result<&GpuImage> {
        let key = image.instance_key();
        if !self.images.contains_key(&key) {
            let gpu_image = unsafe { self.upload(image)? };
            self.images.insert(key.clone(), gpu_image);
        }
        Ok(self.images.get(&key).unwrap())
    }

    pub(super) fn get(&self, image: &Image) -> Option<&GpuImage> {
        self.images.get(&image.instance_key())
    }

    pub(super) fn bind_sampled_view(
        &self,
        view: vk::ImageView,
        layout: vk::ImageLayout,
    ) -> Result<SampledImageBinding> {
        unsafe {
            let pool_sizes = [
                vk::DescriptorPoolSize::default()
                    .ty(vk::DescriptorType::SAMPLER)
                    .descriptor_count(1),
                vk::DescriptorPoolSize::default()
                    .ty(vk::DescriptorType::SAMPLED_IMAGE)
                    .descriptor_count(1),
            ];
            let descriptor_pool = self.device.raw().create_descriptor_pool(
                &vk::DescriptorPoolCreateInfo::default()
                    .max_sets(1)
                    .pool_sizes(&pool_sizes),
                None,
            )?;
            let layouts = [self.descriptor_set_layout];
            let descriptor_set = match self.device.raw().allocate_descriptor_sets(
                &vk::DescriptorSetAllocateInfo::default()
                    .descriptor_pool(descriptor_pool)
                    .set_layouts(&layouts),
            ) {
                Ok(sets) => sets[0],
                Err(error) => {
                    self.device
                        .raw()
                        .destroy_descriptor_pool(descriptor_pool, None);
                    return Err(error.into());
                }
            };
            let sampler_info = [vk::DescriptorImageInfo::default()
                .sampler(self.samplers[&SamplerKey::RENDER_IMAGE])];
            let image_info = [vk::DescriptorImageInfo::default()
                .image_view(view)
                .image_layout(layout)];
            let writes = [
                vk::WriteDescriptorSet::default()
                    .dst_set(descriptor_set)
                    .dst_binding(0)
                    .descriptor_type(vk::DescriptorType::SAMPLER)
                    .image_info(&sampler_info),
                vk::WriteDescriptorSet::default()
                    .dst_set(descriptor_set)
                    .dst_binding(1)
                    .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                    .image_info(&image_info),
            ];
            self.device.raw().update_descriptor_sets(&writes, &[]);
            Ok(SampledImageBinding {
                device: self.device.clone(),
                descriptor_pool,
                descriptor_set,
            })
        }
    }

    unsafe fn upload(&self, source: &Image) -> Result<GpuImage> {
        let device = self.device.raw();
        let byte_size = source.rgba8().len() as vk::DeviceSize;

        let staging_buffer = unsafe {
            device.create_buffer(
                &vk::BufferCreateInfo::default()
                    .size(byte_size)
                    .usage(vk::BufferUsageFlags::TRANSFER_SRC)
                    .sharing_mode(vk::SharingMode::EXCLUSIVE),
                None,
            )?
        };
        let staging_requirements = unsafe { device.get_buffer_memory_requirements(staging_buffer) };
        let staging_memory_type = self
            .device
            .find_memory_type(
                staging_requirements.memory_type_bits,
                vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
            )
            .ok_or("no host-visible coherent Vulkan memory type for image upload")?;
        let staging_memory = unsafe {
            device.allocate_memory(
                &vk::MemoryAllocateInfo::default()
                    .allocation_size(staging_requirements.size)
                    .memory_type_index(staging_memory_type),
                None,
            )?
        };
        unsafe { device.bind_buffer_memory(staging_buffer, staging_memory, 0)? };
        let mapped = unsafe {
            device.map_memory(staging_memory, 0, byte_size, vk::MemoryMapFlags::empty())?
        };
        unsafe {
            ptr::copy_nonoverlapping(source.rgba8().as_ptr(), mapped.cast(), source.rgba8().len());
            device.unmap_memory(staging_memory);
        }

        let extent = vk::Extent3D {
            width: source.width(),
            height: source.height(),
            depth: 1,
        };
        let image = unsafe {
            device.create_image(
                &vk::ImageCreateInfo::default()
                    .image_type(vk::ImageType::TYPE_2D)
                    .format(vk::Format::R8G8B8A8_SRGB)
                    .extent(extent)
                    .mip_levels(1)
                    .array_layers(1)
                    .samples(vk::SampleCountFlags::TYPE_1)
                    .tiling(vk::ImageTiling::OPTIMAL)
                    .usage(vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED)
                    .sharing_mode(vk::SharingMode::EXCLUSIVE)
                    .initial_layout(vk::ImageLayout::UNDEFINED),
                None,
            )?
        };
        let image_requirements = unsafe { device.get_image_memory_requirements(image) };
        let image_memory_type = self
            .device
            .find_memory_type(
                image_requirements.memory_type_bits,
                vk::MemoryPropertyFlags::DEVICE_LOCAL,
            )
            .ok_or("no device-local Vulkan memory type for sampled image")?;
        let memory = unsafe {
            device.allocate_memory(
                &vk::MemoryAllocateInfo::default()
                    .allocation_size(image_requirements.size)
                    .memory_type_index(image_memory_type),
                None,
            )?
        };
        unsafe { device.bind_image_memory(image, memory, 0)? };

        unsafe {
            self.copy_to_image(staging_buffer, image, extent)?;
            device.destroy_buffer(staging_buffer, None);
            device.free_memory(staging_memory, None);
        }

        let view = unsafe {
            device.create_image_view(
                &vk::ImageViewCreateInfo::default()
                    .image(image)
                    .view_type(vk::ImageViewType::TYPE_2D)
                    .format(vk::Format::R8G8B8A8_SRGB)
                    .subresource_range(
                        vk::ImageSubresourceRange::default()
                            .aspect_mask(vk::ImageAspectFlags::COLOR)
                            .level_count(1)
                            .layer_count(1),
                    ),
                None,
            )?
        };

        let descriptor_count = u32::try_from(self.samplers.len())?;
        let pool_sizes = [
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::SAMPLER)
                .descriptor_count(descriptor_count),
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::SAMPLED_IMAGE)
                .descriptor_count(descriptor_count),
        ];
        let descriptor_pool = unsafe {
            device.create_descriptor_pool(
                &vk::DescriptorPoolCreateInfo::default()
                    .max_sets(descriptor_count)
                    .pool_sizes(&pool_sizes),
                None,
            )?
        };
        let layouts = vec![self.descriptor_set_layout; self.samplers.len()];
        let descriptor_sets = unsafe {
            device.allocate_descriptor_sets(
                &vk::DescriptorSetAllocateInfo::default()
                    .descriptor_pool(descriptor_pool)
                    .set_layouts(&layouts),
            )?
        };
        let mut sets_by_sampler = HashMap::new();
        for ((key, sampler), descriptor_set) in self.samplers.iter().zip(descriptor_sets) {
            let sampler_info = [vk::DescriptorImageInfo::default().sampler(*sampler)];
            let image_info = [vk::DescriptorImageInfo::default()
                .image_view(view)
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
            let writes = [
                vk::WriteDescriptorSet::default()
                    .dst_set(descriptor_set)
                    .dst_binding(0)
                    .descriptor_type(vk::DescriptorType::SAMPLER)
                    .image_info(&sampler_info),
                vk::WriteDescriptorSet::default()
                    .dst_set(descriptor_set)
                    .dst_binding(1)
                    .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                    .image_info(&image_info),
            ];
            unsafe { device.update_descriptor_sets(&writes, &[]) };
            sets_by_sampler.insert(*key, descriptor_set);
        }

        Ok(GpuImage {
            image,
            view,
            memory,
            descriptor_pool,
            descriptor_sets: sets_by_sampler,
        })
    }

    unsafe fn copy_to_image(
        &self,
        staging_buffer: vk::Buffer,
        image: vk::Image,
        extent: vk::Extent3D,
    ) -> Result<()> {
        let device = self.device.raw();
        let command_pool = unsafe {
            device.create_command_pool(
                &vk::CommandPoolCreateInfo::default()
                    .flags(vk::CommandPoolCreateFlags::TRANSIENT)
                    .queue_family_index(self.device.queue_family_index()),
                None,
            )?
        };
        let command_buffer = unsafe {
            device.allocate_command_buffers(
                &vk::CommandBufferAllocateInfo::default()
                    .command_pool(command_pool)
                    .level(vk::CommandBufferLevel::PRIMARY)
                    .command_buffer_count(1),
            )?[0]
        };
        unsafe {
            device.begin_command_buffer(
                command_buffer,
                &vk::CommandBufferBeginInfo::default()
                    .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
            )?;
            transition_image(
                device,
                command_buffer,
                image,
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
                .image_extent(extent);
            device.cmd_copy_buffer_to_image(
                command_buffer,
                staging_buffer,
                image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &[region],
            );
            transition_image(
                device,
                command_buffer,
                image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                vk::PipelineStageFlags2::COPY,
                vk::AccessFlags2::TRANSFER_WRITE,
                vk::PipelineStageFlags2::FRAGMENT_SHADER,
                vk::AccessFlags2::SHADER_SAMPLED_READ,
            );
            device.end_command_buffer(command_buffer)?;
        }

        let fence = unsafe { device.create_fence(&vk::FenceCreateInfo::default(), None)? };
        let command_buffers = [command_buffer];
        let submit = vk::SubmitInfo::default().command_buffers(&command_buffers);
        let result = unsafe {
            device
                .queue_submit(self.device.graphics_queue(), &[submit], fence)
                .and_then(|()| device.wait_for_fences(&[fence], true, u64::MAX))
        };
        unsafe {
            device.destroy_fence(fence, None);
            device.destroy_command_pool(command_pool, None);
        }
        result?;
        Ok(())
    }
}

impl Drop for ImageCache {
    fn drop(&mut self) {
        unsafe {
            for gpu_image in self.images.drain().map(|(_, image)| image) {
                self.device
                    .raw()
                    .destroy_descriptor_pool(gpu_image.descriptor_pool, None);
                self.device.raw().destroy_image_view(gpu_image.view, None);
                self.device.raw().destroy_image(gpu_image.image, None);
                self.device.raw().free_memory(gpu_image.memory, None);
            }
            for sampler in self.samplers.drain().map(|(_, sampler)| sampler) {
                self.device.raw().destroy_sampler(sampler, None);
            }
            self.device
                .raw()
                .destroy_descriptor_set_layout(self.descriptor_set_layout, None);
        }
    }
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
