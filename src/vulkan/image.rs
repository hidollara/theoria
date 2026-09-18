use std::{collections::HashMap, error::Error, sync::Arc};

use ash::vk;

use crate::{
    canvas::{
        canvas_image::{CanvasImage, CanvasImageInstanceKey},
        image::{Image, ImageKind},
    },
    image::{Srgba8Image, Srgba8ImageInstanceKey},
};

use super::{color::RgbAlphaAssociation, presenter::RenderDevice};

mod descriptors;
mod storage;
mod upload;

use descriptors::ImageDescriptors;
pub(super) use descriptors::{GpuImage, SampledImageBinding};
pub(super) use storage::VulkanImage;
use upload::upload_srgba8;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

pub(super) struct ImageCache {
    // Fields drop in declaration order: image bindings must disappear before
    // the shared sampler and descriptor-layout infrastructure they reference.
    srgba8: HashMap<Srgba8ImageInstanceKey, GpuImage>,
    canvas: HashMap<CanvasImageInstanceKey, GpuImage>,
    descriptors: ImageDescriptors,
    device: RenderDevice,
}

impl ImageCache {
    pub(super) fn new(device: RenderDevice) -> Result<Self> {
        let descriptors = ImageDescriptors::new(device.clone())?;
        Ok(Self {
            srgba8: HashMap::new(),
            canvas: HashMap::new(),
            descriptors,
            device,
        })
    }

    pub(super) fn descriptor_set_layout(&self) -> vk::DescriptorSetLayout {
        self.descriptors.layout()
    }

    pub(super) fn ensure_uploaded(&mut self, image: &Srgba8Image) -> Result<()> {
        self.prune_unused();
        let key = image.instance_key();
        if !self.srgba8.contains_key(&key) {
            let gpu_image = self.upload(image)?;
            self.srgba8.insert(key, gpu_image);
        }
        Ok(())
    }

    pub(super) fn insert_canvas(
        &mut self,
        image: &CanvasImage,
        backing: Arc<VulkanImage>,
    ) -> Result<()> {
        self.prune_unused();
        let gpu_image = self
            .descriptors
            .bind_backing(backing, RgbAlphaAssociation::Premultiplied)?;
        self.canvas.insert(image.instance_key(), gpu_image);
        Ok(())
    }

    pub(super) fn contains_canvas(&mut self, image: &CanvasImage) -> bool {
        self.prune_unused();
        self.canvas.contains_key(&image.instance_key())
    }

    pub(super) fn get(&self, image: &Image) -> Option<&GpuImage> {
        match image.kind() {
            ImageKind::Srgba8(image) => self.srgba8.get(&image.instance_key()),
            ImageKind::Canvas(image) => self.canvas.get(&image.instance_key()),
        }
    }

    fn prune_unused(&mut self) {
        self.srgba8.retain(|key, _| key.is_alive());
        self.canvas.retain(|key, _| key.is_alive());
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

    fn upload(&self, source: &Srgba8Image) -> Result<GpuImage> {
        let backing = upload_srgba8(self.device.clone(), source)?;
        self.descriptors
            .bind_backing(backing, RgbAlphaAssociation::Straight)
    }
}
