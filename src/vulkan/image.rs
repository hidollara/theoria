use std::{collections::HashMap, error::Error, sync::Arc};

use ash::vk;

use crate::{
    canvas::{
        canvas_image::{CanvasImage, CanvasImageInstanceKey},
        image::{Image, ImageKind, RasterImageSource, RasterImageView},
    },
    canvas_renderer::{icc_image_shader, raster::pack_icc_samples},
    color::ColorSpace,
    image::RasterImageInstanceKey,
};

use super::{color::RgbAlphaAssociation, presenter::RenderDevice};

mod descriptors;
mod storage;
mod upload;

use descriptors::ImageDescriptors;
pub(super) use descriptors::{GpuImage, GpuImageColor, SampledImageBinding};
pub(super) use storage::VulkanImage;
use upload::{upload_rgba8, upload_rgba8_array};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

pub(super) struct ImageCache {
    // Fields drop in declaration order: image bindings must disappear before
    // the shared sampler and descriptor-layout infrastructure they reference.
    raster: HashMap<RasterImageInstanceKey, GpuImage>,
    canvas: HashMap<CanvasImageInstanceKey, GpuImage>,
    descriptors: ImageDescriptors,
    device: RenderDevice,
    working_profile: ColorSpace,
}

impl ImageCache {
    pub(super) fn new(device: RenderDevice, working_profile: ColorSpace) -> Result<Self> {
        let descriptors = ImageDescriptors::new(device.clone())?;
        Ok(Self {
            raster: HashMap::new(),
            canvas: HashMap::new(),
            descriptors,
            device,
            working_profile,
        })
    }

    pub(super) fn descriptor_set_layout(&self) -> vk::DescriptorSetLayout {
        self.descriptors.layout()
    }

    pub(super) fn icc_descriptor_set_layout(&self) -> vk::DescriptorSetLayout {
        self.descriptors.icc_layout()
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
            self.working_profile.clone(),
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

    pub(super) fn active_icc_shaders(
        &mut self,
    ) -> Vec<(
        (
            crate::color::ColorSpace,
            crate::color::IccRenderingIntent,
            bool,
        ),
        Arc<str>,
    )> {
        self.prune_unused();
        self.raster
            .values()
            .filter_map(|image| match &image.color {
                GpuImageColor::Icc { key, shader_source } => {
                    Some((key.clone(), shader_source.clone()))
                }
                GpuImageColor::Rgb { .. } => None,
            })
            .collect()
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
            RasterImageView::Icc8 {
                pixels,
                profile,
                intent,
                has_alpha,
            } => {
                if !self
                    .device
                    .supports_filterable_sampled_format(vk::Format::R8G8B8A8_UNORM)
                {
                    return Err("ICC image sampling requires filterable RGBA8 support".into());
                }
                let packed = pack_icc_samples(
                    source.width(),
                    source.height(),
                    pixels,
                    profile.device_channels(),
                    has_alpha,
                )?;
                let backing = upload_rgba8_array(
                    self.device.clone(),
                    source.width(),
                    source.height(),
                    &packed.samples,
                    packed.layers,
                )?;
                let lowered = icc_image_shader::build(
                    profile,
                    intent,
                    has_alpha,
                    &self.working_profile,
                    include_str!("../../shaders/vulkan/icc_image.wgsl"),
                )?;
                self.descriptors.bind_icc_backing(
                    backing,
                    (profile.profile(), intent, has_alpha),
                    lowered.source,
                    &lowered.tables,
                )
            }
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
