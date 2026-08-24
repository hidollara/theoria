use std::{mem::size_of, slice};

use crate::{
    canvas::{BlendMode, CanvasView, Composite, ImagePattern},
    geometry::Point,
    transform::Projective2,
};

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct BlendCompositePushConstants {
    blend_mode: u32,
    composite: u32,
}

impl BlendCompositePushConstants {
    pub(super) fn new(blend_mode: BlendMode, composite: Composite) -> Self {
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
            blend_mode,
            composite,
        }
    }

    pub(super) fn as_bytes(&self) -> &[u8] {
        unsafe { slice::from_raw_parts((self as *const Self).cast(), size_of::<Self>()) }
    }
}

use super::super::image::GpuAlphaMode;

#[repr(C)]
#[derive(Clone, Copy)]
struct GpuVertex {
    position: [f32; 4],
    color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct TrianglePushConstants {
    vertices: [GpuVertex; 3],
}

impl TrianglePushConstants {
    pub(super) fn new(
        points: [Point; 3],
        colors: [[f32; 4]; 3],
        canvas_to_root: Projective2,
        view: CanvasView,
    ) -> Self {
        let canvas_to_view = canvas_to_root.then(view.canvas_to_view);
        Self {
            vertices: std::array::from_fn(|index| GpuVertex {
                position: transformed_point_to_clip(points[index], canvas_to_view),
                color: colors[index],
            }),
        }
    }

    pub(super) fn as_bytes(&self) -> &[u8] {
        unsafe { slice::from_raw_parts((self as *const Self).cast(), size_of::<Self>()) }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct ImagePushConstants {
    pub(super) vertices: [ImageVertex; 3],
}

impl ImagePushConstants {
    pub(super) fn new(
        pattern: &ImagePattern,
        alpha_mode: GpuAlphaMode,
        points: [Point; 3],
        image_to_root: Projective2,
        view: CanvasView,
    ) -> Self {
        let width = pattern.image().width() as f32;
        let height = pattern.image().height() as f32;
        let premultiplied = match alpha_mode {
            GpuAlphaMode::Straight => 0.0,
            GpuAlphaMode::Premultiplied => 1.0,
        };
        let image_to_view = image_to_root.then(view.canvas_to_view);
        Self {
            vertices: points.map(|point| {
                let image_point = pattern.canvas_to_image().transform_homogeneous(point);
                ImageVertex {
                    position: transformed_point_to_clip(point, image_to_view),
                    uv: [
                        image_point.x / width,
                        image_point.y / height,
                        image_point.z,
                        premultiplied,
                    ],
                }
            }),
        }
    }

    pub(super) fn as_bytes(&self) -> &[u8] {
        unsafe { slice::from_raw_parts((self as *const Self).cast(), size_of::<Self>()) }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct ImageVertex {
    position: [f32; 4],
    pub(super) uv: [f32; 4],
}

fn transformed_point_to_clip(point: Point, canvas_to_view: Projective2) -> [f32; 4] {
    let transformed = canvas_to_view.transform_homogeneous(point);
    [
        transformed.x * 2.0 - transformed.z,
        transformed.z - transformed.y * 2.0,
        0.0,
        transformed.z,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_w3c_blend_mode_has_a_stable_unique_shader_identity() {
        for (expected, blend_mode) in BlendMode::ALL.into_iter().enumerate() {
            let encoded = BlendCompositePushConstants::new(blend_mode, Composite::SourceOver);
            assert_eq!(encoded.blend_mode, expected as u32);
        }
    }
}
