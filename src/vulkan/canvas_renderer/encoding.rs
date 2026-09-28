use std::{mem::size_of, slice};

use crate::{
    canvas::{CanvasView, Composition, ImageExtend, ImagePattern, ImageSampling},
    canvas_renderer::{composition_encoding, view_space::transformed_point_to_clip},
    geometry::Point,
    surface::OutputAlpha,
    transform::Projective2,
};

use super::super::color::PremultipliedWorkingRgba;

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct OutputPushConstants {
    clear_color: [f32; 4],
    alpha_mode: u32,
    _padding: [u32; 3],
}

impl OutputPushConstants {
    pub(super) fn new(clear_color: PremultipliedWorkingRgba, alpha: OutputAlpha) -> Self {
        let alpha_mode = match alpha {
            OutputAlpha::Premultiplied => 0,
            OutputAlpha::Straight => 1,
            OutputAlpha::Opaque => 2,
        };
        Self {
            clear_color: clear_color.components(),
            alpha_mode,
            _padding: [0; 3],
        }
    }

    pub(super) fn as_bytes(&self) -> &[u8] {
        unsafe { slice::from_raw_parts((self as *const Self).cast(), size_of::<Self>()) }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct CompositionPushConstants {
    values: [u32; 2],
}

impl CompositionPushConstants {
    pub(super) fn new(composition: &Composition) -> Self {
        Self {
            values: composition_encoding::encode(composition),
        }
    }

    pub(super) fn as_bytes(&self) -> &[u8] {
        unsafe { slice::from_raw_parts((self as *const Self).cast(), size_of::<Self>()) }
    }
}

use super::super::color::RgbAlphaAssociation;

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
        colors: [PremultipliedWorkingRgba; 3],
        canvas_to_root: Projective2,
        view: CanvasView,
    ) -> Self {
        let canvas_to_view = canvas_to_root.then(view.canvas_to_view);
        Self {
            vertices: std::array::from_fn(|index| GpuVertex {
                position: transformed_point_to_clip(points[index], canvas_to_view),
                color: colors[index].components(),
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
    metadata: [u32; 4],
}

impl ImagePushConstants {
    pub(super) fn new_icc(
        pattern: &ImagePattern,
        points: [Point; 3],
        image_to_root: Projective2,
        view: CanvasView,
    ) -> Self {
        let mut result = Self::new(
            pattern,
            RgbAlphaAssociation::Straight,
            4,
            points,
            image_to_root,
            view,
        );
        result.metadata[0] = u32::from(pattern.sampling() == ImageSampling::Linear)
            | (u32::from(pattern.extend_x() == ImageExtend::Transparent) << 1)
            | (u32::from(pattern.extend_y() == ImageExtend::Transparent) << 2);
        result
    }

    pub(super) fn new(
        pattern: &ImagePattern,
        rgb_alpha_association: RgbAlphaAssociation,
        image_space_index: u32,
        points: [Point; 3],
        image_to_root: Projective2,
        view: CanvasView,
    ) -> Self {
        Self::new_sampled(
            pattern.image().width(),
            pattern.image().height(),
            pattern.canvas_to_image(),
            rgb_alpha_association,
            image_space_index,
            points,
            image_to_root,
            view,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn new_sampled(
        width: u32,
        height: u32,
        canvas_to_image: Projective2,
        rgb_alpha_association: RgbAlphaAssociation,
        image_space_index: u32,
        points: [Point; 3],
        image_to_root: Projective2,
        view: CanvasView,
    ) -> Self {
        let width = width as f32;
        let height = height as f32;
        let rgb_is_premultiplied = match rgb_alpha_association {
            RgbAlphaAssociation::Straight => 0.0,
            RgbAlphaAssociation::Premultiplied => 1.0,
        };
        let image_to_view = image_to_root.then(view.canvas_to_view);
        Self {
            vertices: points.map(|point| {
                let image_point = canvas_to_image.transform_homogeneous(point);
                ImageVertex {
                    position: transformed_point_to_clip(point, image_to_view),
                    uv: [
                        image_point.x / width,
                        image_point.y / height,
                        image_point.z,
                        rgb_is_premultiplied,
                    ],
                }
            }),
            metadata: [image_space_index, 0, 0, 0],
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
