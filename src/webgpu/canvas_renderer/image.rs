use std::collections::HashMap;

use crate::{
    canvas::{
        canvas_image::{CanvasImage, CanvasImageInstanceKey},
        image::{Image, ImageKind},
    },
    image::{Srgba8Image, Srgba8ImageInstanceKey},
};

use super::pipeline::WORKING_FORMAT;

pub(super) struct GpuImage {
    _texture: wgpu::Texture,
    pub(super) view: wgpu::TextureView,
    pub(super) rgb_is_premultiplied: bool,
}

pub(super) struct ImageCache {
    srgba8: HashMap<Srgba8ImageInstanceKey, GpuImage>,
    canvas: HashMap<CanvasImageInstanceKey, GpuImage>,
}

impl ImageCache {
    pub(super) fn new() -> Self {
        Self {
            srgba8: HashMap::new(),
            canvas: HashMap::new(),
        }
    }

    pub(super) fn ensure_uploaded(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        image: &Srgba8Image,
    ) {
        self.prune_unused();
        let key = image.instance_key();
        self.srgba8.entry(key).or_insert_with(|| {
            let size = extent(image.width(), image.height());
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("theoria WebGPU sRGBA8 image"),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                image.pixels(),
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(image.width() * 4),
                    rows_per_image: Some(image.height()),
                },
                size,
            );
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            GpuImage {
                _texture: texture,
                view,
                rgb_is_premultiplied: false,
            }
        });
    }

    pub(super) fn contains_canvas(&mut self, image: &CanvasImage) -> bool {
        self.prune_unused();
        self.canvas.contains_key(&image.instance_key())
    }

    pub(super) fn insert_canvas(&mut self, image: &CanvasImage, backing: GpuImage) {
        self.prune_unused();
        self.canvas.insert(image.instance_key(), backing);
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
}

pub(super) fn create_working_image(
    device: &wgpu::Device,
    width: u32,
    height: u32,
    label: &'static str,
) -> GpuImage {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: extent(width, height),
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: WORKING_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    GpuImage {
        _texture: texture,
        view,
        rgb_is_premultiplied: true,
    }
}

fn extent(width: u32, height: u32) -> wgpu::Extent3d {
    wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    }
}
