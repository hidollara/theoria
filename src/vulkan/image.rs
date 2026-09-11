use std::{collections::HashMap, error::Error, ptr, sync::Arc};

use ash::vk;

use crate::vulkan::image_layout::transition_image;
use crate::{
    canvas::{ImageExtend, ImagePattern, ImageSampling},
    image::{Image, ImageContent, ImageInstanceKey},
};

use super::presenter::RenderDevice;

mod storage;

pub(super) use storage::VulkanImage;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

pub(super) struct GpuImage {
    _backing: Arc<VulkanImage>,
    descriptor_pool: vk::DescriptorPool,
    descriptor_sets: HashMap<SamplerKey, vk::DescriptorSet>,
    alpha_mode: GpuAlphaMode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum GpuAlphaMode {
    Straight,
    Premultiplied,
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

    pub(super) fn alpha_mode(&self) -> GpuAlphaMode {
        self.alpha_mode
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

    pub(super) fn ensure_uploaded(&mut self, image: &Image) -> Result<()> {
        self.prune_unused();
        let key = image.instance_key();
        if !self.images.contains_key(&key) {
            if !matches!(image.content(), ImageContent::Rgba8(_)) {
                return Err("only CPU raster Images can be uploaded directly".into());
            }
            let gpu_image = unsafe { self.upload(image)? };
            self.images.insert(key.clone(), gpu_image);
        }
        Ok(())
    }

    pub(super) fn insert_rendered(
        &mut self,
        image: &Image,
        backing: Arc<VulkanImage>,
    ) -> Result<()> {
        self.prune_unused();
        let gpu_image = unsafe { self.bind_backing(backing, GpuAlphaMode::Premultiplied)? };
        self.images.insert(image.instance_key(), gpu_image);
        Ok(())
    }

    pub(super) fn contains(&mut self, image: &Image) -> bool {
        self.prune_unused();
        self.images.contains_key(&image.instance_key())
    }

    pub(super) fn get(&self, image: &Image) -> Option<&GpuImage> {
        self.images.get(&image.instance_key())
    }

    fn prune_unused(&mut self) {
        self.images.retain(|key, _| key.is_alive());
    }

    pub(super) fn create_render_target(
        &self,
        format: vk::Format,
        extent: vk::Extent2D,
    ) -> Result<Arc<VulkanImage>> {
        Ok(Arc::new(unsafe {
            VulkanImage::new(
                self.device.clone(),
                format,
                extent,
                vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED,
            )?
        }))
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
        let pixels = source
            .rgba8()
            .ok_or("the Image has neither RGBA8 pixels nor a compatible Vulkan materialization")?;
        let byte_size = pixels.len() as vk::DeviceSize;

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
            ptr::copy_nonoverlapping(pixels.as_ptr(), mapped.cast(), pixels.len());
            device.unmap_memory(staging_memory);
        }

        let extent = vk::Extent2D {
            width: source.width(),
            height: source.height(),
        };
        let backing = Arc::new(unsafe {
            VulkanImage::new(
                self.device.clone(),
                vk::Format::R8G8B8A8_SRGB,
                extent,
                vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED,
            )?
        });

        unsafe {
            self.copy_to_image(
                staging_buffer,
                backing.image,
                vk::Extent3D {
                    width: extent.width,
                    height: extent.height,
                    depth: 1,
                },
            )?;
            device.destroy_buffer(staging_buffer, None);
            device.free_memory(staging_memory, None);
        }
        unsafe { self.bind_backing(backing, GpuAlphaMode::Straight) }
    }

    unsafe fn bind_backing(
        &self,
        backing: Arc<VulkanImage>,
        alpha_mode: GpuAlphaMode,
    ) -> Result<GpuImage> {
        let device = self.device.raw();
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
                .image_view(backing.view)
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
            _backing: backing,
            descriptor_pool,
            descriptor_sets: sets_by_sampler,
            alpha_mode,
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
