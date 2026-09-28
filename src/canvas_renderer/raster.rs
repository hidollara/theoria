use std::io;

#[cfg(test)]
use half::f16;

#[cfg(test)]
use crate::color::{IccProfile, IccRenderingIntent, LinearSrgb, connect_spaces};
use crate::image::RasterPlane;

pub(crate) struct PackedIccSamples {
    pub(crate) samples: Vec<u8>,
    pub(crate) layers: u32,
    pub(crate) row_stride: usize,
}

/// Packs unchanged ICC device samples and optional straight alpha into RGBA8
/// texture-array layers. Unused components are padding, not color conversion.
pub(crate) fn pack_icc_samples(
    width: u32,
    height: u32,
    pixels: &[u8],
    device_channels: usize,
    has_alpha: bool,
) -> Result<PackedIccSamples, Box<dyn std::error::Error>> {
    let channels = device_channels + usize::from(has_alpha);
    let layers = u32::try_from(channels.div_ceil(4))?;
    let width = usize::try_from(width)?;
    let height = usize::try_from(height)?;
    let overflow = || io::Error::from(io::ErrorKind::OutOfMemory);
    let row_stride = width.checked_mul(4).ok_or_else(overflow)?;
    let layer_size = row_stride.checked_mul(height).ok_or_else(overflow)?;
    let size = layer_size
        .checked_mul(layers as usize)
        .ok_or_else(overflow)?;
    let mut samples = vec![255; size];
    for (pixel_index, pixel) in pixels.chunks_exact(channels).enumerate() {
        for (channel, sample) in pixel.iter().copied().enumerate() {
            let offset = (channel / 4) * layer_size + pixel_index * 4 + channel % 4;
            samples[offset] = sample;
        }
    }
    Ok(PackedIccSamples {
        samples,
        layers,
        row_stride,
    })
}

/// CPU reference used by renderer tests to check sampled ICC coordinates.
#[cfg(test)]
pub(crate) fn convert_icc8_to_linear_rgba16(
    width: u32,
    pixels: &[u8],
    profile: &IccProfile,
    intent: IccRenderingIntent,
    has_alpha: bool,
) -> Result<RasterPlane<u16>, Box<dyn std::error::Error>> {
    let transfer = connect_spaces(profile.color_space(intent), LinearSrgb)?;
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

/// Expands an RGB8 plane into the portable RGBA8 upload representation used
/// by renderers whose sampled texture API has no three-channel byte format.
pub(crate) fn expand_rgb8_to_rgba8(
    width: u32,
    height: u32,
    plane: &RasterPlane<u8>,
) -> RasterPlane<u8> {
    let width = usize::try_from(width).expect("validated image width fits address space");
    let height = usize::try_from(height).expect("validated image height fits address space");
    let output_stride = width
        .checked_mul(4)
        .expect("validated image dimensions fit address space");
    let mut output = Vec::with_capacity(
        output_stride
            .checked_mul(height)
            .expect("validated image dimensions fit address space"),
    );
    for row in plane.samples().chunks_exact(plane.row_stride()) {
        for pixel in row[..width * 3].chunks_exact(3) {
            output.extend_from_slice(pixel);
            output.push(u8::MAX);
        }
    }
    RasterPlane::new(output_stride, output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icc_upload_preserves_source_channels_and_separate_alpha() {
        let packed =
            pack_icc_samples(2, 1, &[10, 20, 30, 40, 50, 60, 70, 80, 90, 100], 4, true).unwrap();
        assert_eq!(packed.layers, 2);
        assert_eq!(packed.row_stride, 8);
        assert_eq!(&packed.samples[..8], &[10, 20, 30, 40, 60, 70, 80, 90]);
        assert_eq!(
            &packed.samples[8..],
            &[50, 255, 255, 255, 100, 255, 255, 255]
        );
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

    #[test]
    fn rgb_expansion_discards_row_padding_and_supplies_opaque_alpha() {
        let plane = RasterPlane::new(9, vec![1, 2, 3, 4, 5, 6, 90, 91, 92]);

        let expanded = expand_rgb8_to_rgba8(2, 1, &plane);

        assert_eq!(expanded.row_stride(), 8);
        assert_eq!(expanded.samples(), &[1, 2, 3, 255, 4, 5, 6, 255]);
    }
}
