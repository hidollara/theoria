use std::mem::size_of;

use crate::{
    canvas::{CanvasView, ImageExtend, ImagePattern},
    canvas_renderer::view_space::transformed_point_to_clip,
    geometry::Point,
    transform::Projective2,
    webgpu::color::PremultipliedLinearSrgba,
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
        colors: [PremultipliedLinearSrgba; 3],
        canvas_to_root: Projective2,
        view: CanvasView,
    ) -> [Self; 3] {
        let canvas_to_view = canvas_to_root.then(view.canvas_to_view);
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
    transparent_extension: [u32; 2],
}

impl ImageVertex {
    pub(super) fn encode(
        pattern: &ImagePattern,
        rgb_is_premultiplied: bool,
        points: [Point; 3],
        image_to_root: Projective2,
        view: CanvasView,
    ) -> [Self; 3] {
        let width = pattern.image().width() as f32;
        let height = pattern.image().height() as f32;
        let image_to_view = image_to_root.then(view.canvas_to_view);
        points.map(|point| {
            let image_point = pattern.canvas_to_image().transform_homogeneous(point);
            Self {
                position: transformed_point_to_clip(point, image_to_view),
                uvw_and_association: [
                    image_point.x / width,
                    image_point.y / height,
                    image_point.z,
                    u32::from(rgb_is_premultiplied) as f32,
                ],
                transparent_extension: [
                    u32::from(pattern.extend_x() == ImageExtend::Transparent),
                    u32::from(pattern.extend_y() == ImageExtend::Transparent),
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
                    format: wgpu::VertexFormat::Uint32x2,
                    offset: 32,
                    shader_location: 2,
                },
            ],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(super) struct BlendCompositeUniform {
    values: [u32; 4],
}

impl BlendCompositeUniform {
    pub(super) fn new(
        blend_mode: crate::canvas::BlendMode,
        composite: crate::canvas::Composite,
    ) -> Self {
        use crate::canvas::{BlendMode, Composite};

        let blend_mode = match blend_mode {
            BlendMode::Normal => 0,
            BlendMode::Multiply => 1,
            BlendMode::Screen => 2,
            BlendMode::Overlay => 3,
            BlendMode::Darken => 4,
            BlendMode::Lighten => 5,
            BlendMode::ColorDodge => 6,
            BlendMode::ColorBurn => 7,
            BlendMode::HardLight => 8,
            BlendMode::SoftLight => 9,
            BlendMode::Difference => 10,
            BlendMode::Exclusion => 11,
            BlendMode::Hue => 12,
            BlendMode::Saturation => 13,
            BlendMode::Color => 14,
            BlendMode::Luminosity => 15,
        };
        let composite = match composite {
            Composite::Clear => 0,
            Composite::Copy => 1,
            Composite::Destination => 2,
            Composite::SourceOver => 3,
            Composite::DestinationOver => 4,
            Composite::SourceIn => 5,
            Composite::DestinationIn => 6,
            Composite::SourceOut => 7,
            Composite::DestinationOut => 8,
            Composite::SourceAtop => 9,
            Composite::DestinationAtop => 10,
            Composite::Xor => 11,
            Composite::Plus => 12,
        };
        Self {
            values: [blend_mode, composite, 0, 0],
        }
    }
}

pub(super) fn bytes_of<T: bytemuck::Pod>(value: &T) -> &[u8] {
    bytemuck::bytes_of(value)
}

pub(super) fn bytes_of_slice<T: bytemuck::Pod>(values: &[T]) -> &[u8] {
    bytemuck::cast_slice(values)
}
