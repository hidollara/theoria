use half::f16;
use std::io;
use theoria_canvas::image::RasterPlane;
use theoria_color::{IccProfile, IccRenderingIntent, LinearSrgb};

/// CPU reference used by renderer tests to check sampled ICC coordinates.
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

#[test]
fn icc_upload_keeps_out_of_srgb_gamut_coordinates() {
    let profile = IccProfile::from_bytes(include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/crates/theoria-color/src/color/fixtures/DisplayP3-v4.icc"
    )))
    .unwrap();
    let converted = convert_icc8_to_linear_rgba16(
        1,
        &[255, 0, 0],
        &profile,
        IccRenderingIntent::RelativeColorimetric,
        false,
    )
    .unwrap();
    let rgba: [f32; 4] =
        std::array::from_fn(|index| f16::from_bits(converted.samples()[index]).to_f32());
    assert!(rgba[0] > 1.0, "red was clipped: {}", rgba[0]);
    assert!(rgba[1] < 0.0, "green was clipped: {}", rgba[1]);
    assert_eq!(rgba[3], 1.0);
}
