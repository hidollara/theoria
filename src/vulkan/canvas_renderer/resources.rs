//! Lazily-created GPU resources shared across Canvas recordings.

use std::error::Error;

use ash::vk;

use crate::{
    canvas::Composition,
    color::ColorSpace,
    vulkan::{device::RenderDevice, image::ImageCache, scratch_image::ScratchImage},
};

use super::pipeline::{Pipelines, create_pipelines};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

pub(super) struct CanvasGpuResources {
    pub(super) pipelines: Option<Pipelines>,
    pub(super) image_cache: Option<ImageCache>,
    pub(super) scratch_images: Vec<ScratchImage>,
}

impl CanvasGpuResources {
    pub(super) fn new() -> Self {
        Self {
            pipelines: None,
            image_cache: None,
            scratch_images: Vec::new(),
        }
    }

    pub(super) fn ensure_image_cache(
        &mut self,
        device: &RenderDevice,
        working_space: &ColorSpace,
    ) -> Result<()> {
        if self.image_cache.is_none() {
            self.image_cache = Some(ImageCache::new(device.clone(), working_space.clone())?);
        }
        Ok(())
    }

    pub(super) fn ensure_pipelines(
        &mut self,
        device: &RenderDevice,
        working_format: vk::Format,
        output_format: vk::Format,
        sample_count: vk::SampleCountFlags,
        working_space: &ColorSpace,
    ) -> Result<()> {
        if self.pipelines.as_ref().is_some_and(|pipelines| {
            pipelines.working_format == working_format
                && pipelines.output_format == output_format
                && pipelines.sample_count == sample_count
                && &pipelines.working_space == working_space
        }) {
            return Ok(());
        }
        self.destroy_pipelines(device);
        let descriptor_layout = self.image_cache.as_ref().unwrap().descriptor_set_layout();
        self.pipelines = Some(unsafe {
            create_pipelines(
                device.raw(),
                working_format,
                output_format,
                sample_count,
                descriptor_layout,
                working_space,
            )?
        });
        Ok(())
    }

    pub(super) fn ensure_scratch_images(
        &mut self,
        device: &RenderDevice,
        format: vk::Format,
        extent: vk::Extent2D,
        sample_count: vk::SampleCountFlags,
        required: usize,
    ) -> Result<()> {
        if self.scratch_images.first().is_some_and(|image| {
            image.format != format || image.extent != extent || image.sample_count != sample_count
        }) {
            self.scratch_images.clear();
        }
        while self.scratch_images.len() < required {
            self.scratch_images.push(ScratchImage::new(
                device.clone(),
                self.image_cache.as_ref().unwrap(),
                format,
                extent,
                sample_count,
            )?);
        }
        Ok(())
    }

    pub(super) fn ensure_compositions(
        &mut self,
        device: &RenderDevice,
        compositions: &[Composition],
    ) -> Result<()> {
        let descriptor_layout = self.image_cache.as_ref().unwrap().descriptor_set_layout();
        unsafe {
            self.pipelines.as_mut().unwrap().ensure_compositions(
                device.raw(),
                descriptor_layout,
                compositions,
            )
        }
    }

    pub(super) fn destroy(&mut self, device: &RenderDevice) {
        self.scratch_images.clear();
        self.destroy_pipelines(device);
        self.image_cache = None;
    }

    fn destroy_pipelines(&mut self, device: &RenderDevice) {
        if let Some(pipelines) = self.pipelines.take() {
            unsafe {
                for pipeline in [pipelines.triangle, pipelines.image, pipelines.mask]
                    .into_iter()
                    .chain(pipelines.composites.into_values())
                    .chain(pipelines.compositions.into_values())
                    .chain(pipelines.outputs.into_values())
                    .chain(pipelines.icc_images.into_values())
                {
                    device.raw().destroy_pipeline(pipeline.handle, None);
                    device.raw().destroy_pipeline_layout(pipeline.layout, None);
                }
            }
        }
    }
}
