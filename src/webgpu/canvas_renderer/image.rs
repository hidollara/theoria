use std::collections::HashMap;

use crate::{
    canvas::{
        canvas_image::{CanvasImage, CanvasImageInstanceKey},
        image::{Image, ImageKind, RasterImageSource, RasterImageView},
    },
    canvas_renderer::raster::{PackedIccSamples, pack_icc_samples},
    color::{ColorSpaceKey, IccRenderingIntent, LinearSrgb, RgbColorSpace},
    image::{AlphaAssociation, RasterImageInstanceKey},
};

use super::pipeline::WORKING_FORMAT;

pub(super) struct GpuImage {
    _texture: wgpu::Texture,
    pub(super) view: wgpu::TextureView,
    pub(super) color: GpuImageColor,
}

pub(super) enum GpuImageColor {
    Rgb {
        premultiplied: bool,
        space: RgbColorSpace,
    },
    Icc {
        key: (ColorSpaceKey, IccRenderingIntent, bool),
    },
}

enum UploadPlane<'a> {
    Borrowed(&'a crate::image::RasterPlane<u8>),
    Expanded(crate::image::RasterPlane<u8>),
    Icc(PackedIccSamples),
}

impl UploadPlane<'_> {
    fn bytes(&self) -> &[u8] {
        match self {
            Self::Borrowed(plane) => plane.samples(),
            Self::Expanded(plane) => plane.samples(),
            Self::Icc(packed) => &packed.samples,
        }
    }

    fn row_stride_bytes(&self) -> usize {
        match self {
            Self::Borrowed(plane) => plane.row_stride(),
            Self::Expanded(plane) => plane.row_stride(),
            Self::Icc(packed) => packed.row_stride,
        }
    }

    fn format(&self) -> wgpu::TextureFormat {
        match self {
            Self::Borrowed(_) | Self::Expanded(_) | Self::Icc(_) => wgpu::TextureFormat::Rgba8Unorm,
        }
    }

    fn layers(&self) -> u32 {
        match self {
            Self::Icc(packed) => packed.layers,
            _ => 1,
        }
    }
}

pub(super) struct ImageCache {
    raster: HashMap<RasterImageInstanceKey, GpuImage>,
    canvas: HashMap<CanvasImageInstanceKey, GpuImage>,
}

impl ImageCache {
    pub(super) fn new() -> Self {
        Self {
            raster: HashMap::new(),
            canvas: HashMap::new(),
        }
    }

    pub(super) fn ensure_uploaded(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        image: &dyn RasterImageSource,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.prune_unused();
        let key = image.instance_key();
        if self.raster.contains_key(&key) {
            return Ok(());
        }
        let (plane, color) = match image.view() {
            RasterImageView::Icc8 {
                pixels,
                profile,
                intent,
                has_alpha,
            } => (
                UploadPlane::Icc(pack_icc_samples(
                    image.width(),
                    image.height(),
                    pixels,
                    profile.device_channels(),
                    has_alpha,
                )?),
                GpuImageColor::Icc {
                    key: (profile.device_space(), intent, has_alpha),
                },
            ),
            RasterImageView::Rgb8 { plane, color_space } => {
                let expanded = crate::canvas_renderer::raster::expand_rgb8_to_rgba8(
                    image.width(),
                    image.height(),
                    plane,
                );
                (
                    UploadPlane::Expanded(expanded),
                    GpuImageColor::Rgb {
                        premultiplied: false,
                        space: color_space,
                    },
                )
            }
            RasterImageView::Rgba8 {
                plane,
                color_space,
                alpha,
            } => (
                UploadPlane::Borrowed(plane),
                GpuImageColor::Rgb {
                    premultiplied: alpha == AlphaAssociation::Premultiplied,
                    space: color_space,
                },
            ),
        };
        let mut size = extent(image.width(), image.height());
        size.depth_or_array_layers = plane.layers();
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("theoria WebGPU raster image"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: plane.format(),
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
            plane.bytes(),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(u32::try_from(plane.row_stride_bytes()).unwrap()),
                rows_per_image: Some(image.height()),
            },
            size,
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: matches!(color, GpuImageColor::Icc { .. })
                .then_some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        let uploaded = GpuImage {
            _texture: texture,
            view,
            color,
        };
        self.raster.insert(key, uploaded);
        Ok(())
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
            ImageKind::Raster(image) => self.raster.get(&image.instance_key()),
            ImageKind::Canvas(image) => self.canvas.get(&image.instance_key()),
        }
    }

    fn prune_unused(&mut self) {
        self.raster.retain(|key, _| key.is_alive());
        self.canvas.retain(|key, _| key.is_alive());
    }

    pub(super) fn active_icc_keys(
        &mut self,
    ) -> std::collections::HashSet<(ColorSpaceKey, IccRenderingIntent, bool)> {
        self.prune_unused();
        self.raster
            .values()
            .filter_map(|image| match &image.color {
                GpuImageColor::Icc { key } => Some(key.clone()),
                GpuImageColor::Rgb { .. } => None,
            })
            .collect()
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
        color: GpuImageColor::Rgb {
            premultiplied: true,
            space: LinearSrgb.into(),
        },
    }
}

fn extent(width: u32, height: u32) -> wgpu::Extent3d {
    wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    }
}
