//! Shared rendering implementation, without backend resources.
#[cfg(test)]
pub(crate) use theoria_renderer::composition_shader;
pub(crate) use theoria_renderer::{
    WorkingColor, composition_encoding, icc_image_shader, mesh_paint, path_sampling, view_space,
};
pub(crate) mod blend {
    pub(crate) use theoria_renderer::{blend::BlendShaderExt, composition::CompositionExt};
}
pub(crate) mod canvas_plan {
    pub(crate) use theoria_renderer::canvas_plan::*;
    #[cfg(test)]
    pub(crate) fn required_scratch_images(canvas: &theoria_canvas::canvas::Canvas) -> usize {
        required_scratch_images_in(canvas, &theoria_color::LinearSrgb.into())
    }
    #[cfg(test)]
    pub(crate) fn can_record_directly(canvas: &theoria_canvas::canvas::Canvas) -> bool {
        can_record_directly_in(canvas, &theoria_color::LinearSrgb.into())
    }
}
pub(crate) mod color_space {
    pub(crate) use theoria_renderer::color_space::*;
    #[cfg(test)]
    pub(crate) fn lower_straight_color(color: &theoria_canvas::canvas::Color) -> [f32; 4] {
        lower_straight_color_in(color, theoria_color::LinearSrgb)
    }
}
pub(crate) mod raster {
    #[cfg(test)]
    use half::f16;
    #[cfg(test)]
    use std::io;
    #[cfg(test)]
    use theoria_canvas::image::RasterPlane;
    #[cfg(test)]
    use theoria_color::{IccProfile, IccRenderingIntent, LinearSrgb};
    pub(crate) use theoria_renderer::raster::*;
    #[cfg(test)]
    pub(crate) fn convert_icc8_to_linear_rgba16(
        width: u32,
        pixels: &[u8],
        profile: &IccProfile,
        intent: IccRenderingIntent,
        has_alpha: bool,
    ) -> Result<RasterPlane<u16>, Box<dyn std::error::Error>> {
        let transfer = profile.color_space(intent).transfer_to(LinearSrgb)?;
        let channels = profile.device_channels();
        let mut output = Vec::with_capacity(pixels.len() / (channels + usize::from(has_alpha)) * 4);
        for pixel in pixels.chunks_exact(channels + usize::from(has_alpha)) {
            let coordinates: Vec<_> = pixel[..channels]
                .iter()
                .map(|&sample| f64::from(sample) / 255.0)
                .collect();
            let rgb = transfer.evaluate(&coordinates)?;
            for &sample in &rgb[..3] {
                let sample = sample as f32;
                if !sample.is_finite() || sample.abs() > f32::from(f16::MAX) {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "ICC color is outside the finite RGBA16F reference range",
                    )
                    .into());
                }
                output.push(f16::from_f32(sample).to_bits());
            }
            let alpha = if has_alpha {
                f32::from(pixel[channels]) / 255.0
            } else {
                1.0
            };
            output.push(f16::from_f32(alpha).to_bits());
        }
        Ok(RasterPlane::new(usize::try_from(width)? * 4, output))
    }
}
