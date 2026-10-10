use std::io;

use half::f16;
use theoria_canvas::{Canvas, image::RasterPlane};
use theoria_color::{IccProfile, IccRenderingIntent, LinearSrgb};
use theoria_renderer::canvas_plan::{can_record_directly_in, required_scratch_images_in};

pub(crate) fn required_scratch_images(canvas: &Canvas) -> usize {
    required_scratch_images_in(canvas, &LinearSrgb.into())
}

pub(crate) fn can_record_directly(canvas: &Canvas) -> bool {
    can_record_directly_in(canvas, &LinearSrgb.into())
}

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
