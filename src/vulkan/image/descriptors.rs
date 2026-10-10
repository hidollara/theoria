//! Samplers and descriptor bindings for sampled Vulkan images.

use std::{collections::HashMap, error::Error, sync::Arc};

use ash::vk;

use crate::{
    canvas::{ImageExtend, ImagePattern, ImageSampling},
    color::ColorSpace,
    vulkan::{
        color::RgbAlphaAssociation, device::RenderDevice, host_buffer::HostBuffer,
        image::storage::VulkanImage,
    },
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

pub(in crate::vulkan) struct GpuImage {
    _descriptor_pool: DescriptorPool,
    descriptor_sets: HashMap<SamplerKey, vk::DescriptorSet>,
    pub(in crate::vulkan) color: GpuImageColor,
    _tables: Option<HostBuffer>,
    // Fields drop in declaration order, so the sampled view outlives its pool.
    _backing: Arc<VulkanImage>,
}

pub(in crate::vulkan) enum GpuImageColor {
    Rgb {
        association: RgbAlphaAssociation,
        space: ColorSpace,
    },
    Icc {
        key: (ColorSpace, bool),
        shader_source: Arc<str>,
    },
}

pub(in crate::vulkan) struct SampledImageBinding {
    _descriptor_pool: DescriptorPool,
    pub(in crate::vulkan) descriptor_set: vk::DescriptorSet,
}

pub(super) struct ImageDescriptors {
    device: RenderDevice,
    samplers: HashMap<SamplerKey, vk::Sampler>,
    layout: vk::DescriptorSetLayout,
    icc_layout: vk::DescriptorSetLayout,
}

struct DescriptorPool {
    device: RenderDevice,
    handle: vk::DescriptorPool,
}

impl DescriptorPool {
    fn new(device: RenderDevice, handle: vk::DescriptorPool) -> Self {
        Self { device, handle }
    }

    fn handle(&self) -> vk::DescriptorPool {
        self.handle
    }
}

impl Drop for DescriptorPool {
    fn drop(&mut self) {
        unsafe { self.device.raw().destroy_descriptor_pool(self.handle, None) };
    }
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
    pub(in crate::vulkan) fn descriptor_set(&self, pattern: &ImagePattern) -> vk::DescriptorSet {
        self.descriptor_sets[&SamplerKey::from_pattern(pattern)]
    }
}

impl ImageDescriptors {
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
            let layout = device.raw().create_descriptor_set_layout(
                &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
                None,
            )?;
            let icc_bindings = [
                bindings[0],
                bindings[1],
                vk::DescriptorSetLayoutBinding::default()
                    .binding(2)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::FRAGMENT),
            ];
            let icc_layout = match device.raw().create_descriptor_set_layout(
                &vk::DescriptorSetLayoutCreateInfo::default().bindings(&icc_bindings),
                None,
            ) {
                Ok(layout) => layout,
                Err(error) => {
                    device.raw().destroy_descriptor_set_layout(layout, None);
                    return Err(error.into());
                }
            };
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
                                device.raw().destroy_descriptor_set_layout(layout, None);
                                device.raw().destroy_descriptor_set_layout(icc_layout, None);
                                return Err(error.into());
                            }
                        }
                    }
                }
            }
            Ok(Self {
                device,
                samplers,
                layout,
                icc_layout,
            })
        }
    }

    pub(super) fn layout(&self) -> vk::DescriptorSetLayout {
        self.layout
    }

    pub(super) fn icc_layout(&self) -> vk::DescriptorSetLayout {
        self.icc_layout
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
            let descriptor_pool = DescriptorPool::new(
                self.device.clone(),
                self.device.raw().create_descriptor_pool(
                    &vk::DescriptorPoolCreateInfo::default()
                        .max_sets(1)
                        .pool_sizes(&pool_sizes),
                    None,
                )?,
            );
            let layouts = [self.layout];
            let descriptor_set = self.device.raw().allocate_descriptor_sets(
                &vk::DescriptorSetAllocateInfo::default()
                    .descriptor_pool(descriptor_pool.handle())
                    .set_layouts(&layouts),
            )?[0];
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
                _descriptor_pool: descriptor_pool,
                descriptor_set,
            })
        }
    }

    pub(super) fn bind_backing(
        &self,
        backing: Arc<VulkanImage>,
        rgb_alpha_association: RgbAlphaAssociation,
        rgb_space: ColorSpace,
    ) -> Result<GpuImage> {
        self.bind_backing_with_color(
            backing,
            GpuImageColor::Rgb {
                association: rgb_alpha_association,
                space: rgb_space,
            },
            None,
        )
    }

    pub(super) fn bind_icc_backing(
        &self,
        backing: Arc<VulkanImage>,
        key: (ColorSpace, bool),
        shader_source: String,
        table_values: &[f32],
    ) -> Result<GpuImage> {
        let tables = HostBuffer::new(
            self.device.clone(),
            u64::try_from(table_values.len() * 4)?,
            vk::BufferUsageFlags::STORAGE_BUFFER,
        )?;
        tables.write(bytemuck::cast_slice(table_values))?;
        self.bind_backing_with_color(
            backing,
            GpuImageColor::Icc {
                key,
                shader_source: shader_source.into(),
            },
            Some(tables),
        )
    }

    fn bind_backing_with_color(
        &self,
        backing: Arc<VulkanImage>,
        color: GpuImageColor,
        tables: Option<HostBuffer>,
    ) -> Result<GpuImage> {
        let device = self.device.raw();
        let descriptor_count = u32::try_from(self.samplers.len())?;
        let mut pool_sizes = vec![
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::SAMPLER)
                .descriptor_count(descriptor_count),
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::SAMPLED_IMAGE)
                .descriptor_count(descriptor_count),
        ];
        if tables.is_some() {
            pool_sizes.push(
                vk::DescriptorPoolSize::default()
                    .ty(vk::DescriptorType::STORAGE_BUFFER)
                    .descriptor_count(descriptor_count),
            );
        }
        let descriptor_pool = DescriptorPool::new(self.device.clone(), unsafe {
            device.create_descriptor_pool(
                &vk::DescriptorPoolCreateInfo::default()
                    .max_sets(descriptor_count)
                    .pool_sizes(&pool_sizes),
                None,
            )?
        });
        let layouts = vec![
            if tables.is_some() {
                self.icc_layout
            } else {
                self.layout
            };
            self.samplers.len()
        ];
        let descriptor_sets = unsafe {
            device.allocate_descriptor_sets(
                &vk::DescriptorSetAllocateInfo::default()
                    .descriptor_pool(descriptor_pool.handle())
                    .set_layouts(&layouts),
            )?
        };
        let mut sets_by_sampler = HashMap::new();
        for ((key, sampler), descriptor_set) in self.samplers.iter().zip(descriptor_sets) {
            let sampler = if tables.is_some() {
                self.samplers[&SamplerKey {
                    sampling: ImageSampling::Nearest,
                    ..*key
                }]
            } else {
                *sampler
            };
            let sampler_info = [vk::DescriptorImageInfo::default().sampler(sampler)];
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
            if let Some(tables) = &tables {
                let info = [vk::DescriptorBufferInfo::default()
                    .buffer(tables.raw())
                    .range(vk::WHOLE_SIZE)];
                let write = [vk::WriteDescriptorSet::default()
                    .dst_set(descriptor_set)
                    .dst_binding(2)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .buffer_info(&info)];
                unsafe { device.update_descriptor_sets(&write, &[]) };
            }
            sets_by_sampler.insert(*key, descriptor_set);
        }

        Ok(GpuImage {
            _descriptor_pool: descriptor_pool,
            descriptor_sets: sets_by_sampler,
            color,
            _tables: tables,
            _backing: backing,
        })
    }
}

impl Drop for ImageDescriptors {
    fn drop(&mut self) {
        unsafe {
            for sampler in self.samplers.drain().map(|(_, sampler)| sampler) {
                self.device.raw().destroy_sampler(sampler, None);
            }
            self.device
                .raw()
                .destroy_descriptor_set_layout(self.layout, None);
            self.device
                .raw()
                .destroy_descriptor_set_layout(self.icc_layout, None);
        }
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
