use std::{mem::size_of, slice};

use crate::{
    canvas::{CanvasView, ImagePattern},
    geometry::Point,
    transform::Projective2,
};

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
