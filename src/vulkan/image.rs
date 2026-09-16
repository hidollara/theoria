use std::{collections::HashMap, error::Error, sync::Arc};

use ash::vk;

use crate::{
    canvas::{Layer, LayerInstanceKey},
    image::{Image, ImageInstanceKey},
};

use super::presenter::RenderDevice;

mod descriptors;
mod storage;
mod upload;

use descriptors::ImageDescriptors;
pub(super) use descriptors::{GpuAlphaMode, GpuImage, SampledImageBinding};
pub(super) use storage::VulkanImage;
use upload::upload_rgba8;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

pub(super) struct ImageCache {
    // Fields drop in declaration order: image bindings must disappear before
    // the shared sampler and descriptor-layout infrastructure they reference.
    images: HashMap<ImageInstanceKey, GpuImage>,
    layers: HashMap<LayerInstanceKey, GpuImage>,
    descriptors: ImageDescriptors,
    device: RenderDevice,
}

impl ImageCache {
    pub(super) fn new(device: RenderDevice) -> Result<Self> {
        let descriptors = ImageDescriptors::new(device.clone())?;
        Ok(Self {
            images: HashMap::new(),
            layers: HashMap::new(),
            descriptors,
            device,
        })
    }

    pub(super) fn descriptor_set_layout(&self) -> vk::DescriptorSetLayout {
        self.descriptors.layout()
    }

    pub(super) fn ensure_uploaded(&mut self, image: &Image) -> Result<()> {
        self.prune_unused();
        let key = image.instance_key();
        if !self.images.contains_key(&key) {
            let gpu_image = self.upload(image)?;
            self.images.insert(key, gpu_image);
        }
        Ok(())
    }

    pub(super) fn insert_layer(&mut self, layer: &Layer, backing: Arc<VulkanImage>) -> Result<()> {
        self.prune_unused();
        let gpu_image = self
            .descriptors
            .bind_backing(backing, GpuAlphaMode::Premultiplied)?;
        self.layers.insert(layer.instance_key(), gpu_image);
        Ok(())
    }

    pub(super) fn contains_layer(&mut self, layer: &Layer) -> bool {
        self.prune_unused();
        self.layers.contains_key(&layer.instance_key())
    }

    pub(super) fn get_layer(&self, layer: &Layer) -> Option<&GpuImage> {
        self.layers.get(&layer.instance_key())
    }

    pub(super) fn get(&self, image: &Image) -> Option<&GpuImage> {
        self.images.get(&image.instance_key())
    }

    fn prune_unused(&mut self) {
        self.images.retain(|key, _| key.is_alive());
        self.layers.retain(|key, _| key.is_alive());
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
            vk::SampleCountFlags::TYPE_1,
            vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED,
        )?))
    }

    pub(super) fn bind_sampled_view(
        &self,
        view: vk::ImageView,
        layout: vk::ImageLayout,
    ) -> Result<SampledImageBinding> {
        self.descriptors.bind_sampled_view(view, layout)
    }

    fn upload(&self, source: &Image) -> Result<GpuImage> {
        let backing = upload_rgba8(self.device.clone(), source)?;
        self.descriptors
            .bind_backing(backing, GpuAlphaMode::Straight)
    }
}
