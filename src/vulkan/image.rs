use std::{collections::HashMap, error::Error, sync::Arc};

use ash::vk;

use crate::{
    canvas::{ImageExtend, ImagePattern, ImageSampling},
    image::{Image, ImageContent, ImageInstanceKey},
};

use super::presenter::RenderDevice;

mod storage;
mod upload;

pub(super) use storage::VulkanImage;
use upload::upload_rgba8;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

pub(super) struct GpuImage {
    _descriptor_pool: DescriptorPool,
    descriptor_sets: HashMap<SamplerKey, vk::DescriptorSet>,
    alpha_mode: GpuAlphaMode,
    // Fields drop in declaration order, so the sampled view outlives its pool.
    _backing: Arc<VulkanImage>,
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
    _descriptor_pool: DescriptorPool,
    pub(super) descriptor_set: vk::DescriptorSet,
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
            let gpu_image = self.upload(image)?;
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
        let gpu_image = self.bind_backing(backing, GpuAlphaMode::Premultiplied)?;
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
        Ok(Arc::new(VulkanImage::new(
            self.device.clone(),
            format,
            extent,
            vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED,
        )?))
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
            let layouts = [self.descriptor_set_layout];
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

    fn upload(&self, source: &Image) -> Result<GpuImage> {
        let backing = upload_rgba8(self.device.clone(), source)?;
        self.bind_backing(backing, GpuAlphaMode::Straight)
    }

    fn bind_backing(
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
        let descriptor_pool = DescriptorPool::new(self.device.clone(), unsafe {
            device.create_descriptor_pool(
                &vk::DescriptorPoolCreateInfo::default()
                    .max_sets(descriptor_count)
                    .pool_sizes(&pool_sizes),
                None,
            )?
        });
        let layouts = vec![self.descriptor_set_layout; self.samplers.len()];
        let descriptor_sets = unsafe {
            device.allocate_descriptor_sets(
                &vk::DescriptorSetAllocateInfo::default()
                    .descriptor_pool(descriptor_pool.handle())
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
            _descriptor_pool: descriptor_pool,
            descriptor_sets: sets_by_sampler,
            alpha_mode,
            _backing: backing,
        })
    }
}

impl Drop for ImageCache {
    fn drop(&mut self) {
        unsafe {
            self.images.clear();
            for sampler in self.samplers.drain().map(|(_, sampler)| sampler) {
                self.device.raw().destroy_sampler(sampler, None);
            }
            self.device
                .raw()
                .destroy_descriptor_set_layout(self.descriptor_set_layout, None);
        }
    }
}
