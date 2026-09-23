use std::{collections::HashMap, error::Error, sync::Arc};

use ash::vk;

use crate::{
    canvas::{
        canvas_image::{CanvasImage, CanvasImageInstanceKey},
        image::{Image, ImageKind, RasterImageSource, RasterImageView},
    },
    color::LinearSrgb,
    image::RasterImageInstanceKey,
};

use super::{color::RgbAlphaAssociation, presenter::RenderDevice};

mod descriptors;
mod storage;
mod upload;

use descriptors::ImageDescriptors;
pub(super) use descriptors::{GpuImage, SampledImageBinding};
pub(super) use storage::VulkanImage;
use upload::upload_rgba8;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

pub(super) struct ImageCache {
    // Fields drop in declaration order: image bindings must disappear before
    // the shared sampler and descriptor-layout infrastructure they reference.
    raster: HashMap<RasterImageInstanceKey, GpuImage>,
    canvas: HashMap<CanvasImageInstanceKey, GpuImage>,
    descriptors: ImageDescriptors,
    device: RenderDevice,
}

impl ImageCache {
    pub(super) fn new(device: RenderDevice) -> Result<Self> {
        let descriptors = ImageDescriptors::new(device.clone())?;
        Ok(Self {
            raster: HashMap::new(),
            canvas: HashMap::new(),
            descriptors,
            device,
        })
    }

    pub(super) fn descriptor_set_layout(&self) -> vk::DescriptorSetLayout {
        self.descriptors.layout()
    }

    pub(super) fn ensure_uploaded(&mut self, image: &dyn RasterImageSource) -> Result<()> {
        self.prune_unused();
        let key = image.instance_key();
        if !self.raster.contains_key(&key) {
            let gpu_image = self.upload(image)?;
            self.raster.insert(key, gpu_image);
        }
        Ok(())
    }

    pub(super) fn insert_canvas(
        &mut self,
        image: &CanvasImage,
        backing: Arc<VulkanImage>,
    ) -> Result<()> {
        self.prune_unused();
        let gpu_image = self.descriptors.bind_backing(
            backing,
            RgbAlphaAssociation::Premultiplied,
            LinearSrgb.into(),
        )?;
        self.canvas.insert(image.instance_key(), gpu_image);
        Ok(())
    }

    pub(super) fn contains_canvas(&mut self, image: &CanvasImage) -> bool {
        self.prune_unused();
        self.canvas.contains_key(&image.instance_key())
    }

    pub(super) fn get(&self, image: &Image) -> Option<&GpuImage> {
        match image.kind() {
            ImageKind::Raster(image) => self.raster.get(&image.instance_key()),
            ImageKind::Canvas(image) => self.canvas.get(&image.instance_key()),
        }
    }

    fn prune_unused(&mut self) {
        self.raster.retain(|key, _| key.is_alive());
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

    fn upload(&self, source: &dyn RasterImageSource) -> Result<GpuImage> {
        match source.view() {
            RasterImageView::Rgb8 { plane, color_space } => {
                let plane = crate::canvas_renderer::raster::expand_rgb8_to_rgba8(
                    source.width(),
                    source.height(),
                    plane,
                );
                let backing =
                    upload_rgba8(self.device.clone(), source.width(), source.height(), &plane)?;
                self.descriptors
                    .bind_backing(backing, RgbAlphaAssociation::Straight, color_space)
            }
            RasterImageView::Rgba8 {
                plane,
                color_space,
                alpha,
            } => {
                let backing =
                    upload_rgba8(self.device.clone(), source.width(), source.height(), plane)?;
                let association = match alpha {
                    crate::image::AlphaAssociation::Straight => RgbAlphaAssociation::Straight,
                    crate::image::AlphaAssociation::Premultiplied => {
                        RgbAlphaAssociation::Premultiplied
                    }
                };
                self.descriptors
                    .bind_backing(backing, association, color_space)
            }
        }
    }
}
