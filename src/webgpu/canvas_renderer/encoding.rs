use std::mem::size_of;

use crate::{
    canvas::{CanvasView, ImageExtend, ImagePattern},
    canvas_renderer::view_space::transformed_point_to_clip,
    geometry::Point,
    surface::OutputAlpha,
    transform::Projective2,
    webgpu::color::PremultipliedWorkingRgba,
};

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(super) struct TriangleVertex {
    position: [f32; 4],
    color: [f32; 4],
}

impl TriangleVertex {
    pub(super) fn encode(
        points: [Point; 3],
        colors: [PremultipliedWorkingRgba; 3],
        canvas_to_root: Projective2,
        view: CanvasView,
    ) -> [Self; 3] {
        let canvas_to_view = canvas_to_root
            .then(view.canvas_to_view)
            .expect("finite composed transform");
        std::array::from_fn(|index| Self {
            position: transformed_point_to_clip(points[index], canvas_to_view),
            color: colors[index].components(),
        })
    }

    pub(super) const fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 0,
                    shader_location: 0,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 16,
                    shader_location: 1,
                },
            ],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(super) struct ImageVertex {
    position: [f32; 4],
    uvw_and_association: [f32; 4],
    metadata: [u32; 4],
}

impl ImageVertex {
    pub(super) fn encode(
        pattern: &ImagePattern,
        rgb_is_premultiplied: bool,
        image_space_index: u32,
        points: [Point; 3],
        image_to_root: Projective2,
        view: CanvasView,
    ) -> [Self; 3] {
        let width = pattern.image().width() as f32;
        let height = pattern.image().height() as f32;
        let image_to_view = image_to_root
            .then(view.canvas_to_view)
            .expect("finite composed transform");
        points.map(|point| {
            let image_point = pattern.canvas_to_image().transform_homogeneous(point);
            Self {
                position: transformed_point_to_clip(point, image_to_view),
                uvw_and_association: [
                    image_point.x / width,
                    image_point.y / height,
                    image_point.w,
                    u32::from(rgb_is_premultiplied) as f32,
                ],
                metadata: [
                    u32::from(pattern.extend_x() == ImageExtend::Transparent),
                    u32::from(pattern.extend_y() == ImageExtend::Transparent),
                    image_space_index,
                    u32::from(pattern.sampling() == crate::canvas::ImageSampling::Linear),
                ],
            }
        })
    }

    pub(super) const fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 0,
                    shader_location: 0,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 16,
                    shader_location: 1,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Uint32x4,
                    offset: 32,
                    shader_location: 2,
                },
            ],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(super) struct CompositionUniform {
    values: [u32; 2],
}

impl CompositionUniform {
    pub(super) fn new(composition: &crate::canvas::Composition) -> Self {
        Self {
            values: crate::canvas_renderer::composition_encoding::encode(composition),
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(super) struct OutputUniform {
    clear_color: [f32; 4],
    alpha_mode: u32,
    _padding: [u32; 3],
}

impl OutputUniform {
    pub(super) fn new(clear_color: [f32; 4], alpha: OutputAlpha) -> Self {
        let alpha_mode = match alpha {
            OutputAlpha::Premultiplied => 0,
            OutputAlpha::Straight => 1,
            OutputAlpha::Opaque => 2,
        };
        Self {
            clear_color,
            alpha_mode,
            _padding: [0; 3],
        }
    }
}

pub(super) fn bytes_of<T: bytemuck::Pod>(value: &T) -> &[u8] {
    bytemuck::bytes_of(value)
}

pub(super) fn bytes_of_slice<T: bytemuck::Pod>(values: &[T]) -> &[u8] {
    bytemuck::cast_slice(values)
}
