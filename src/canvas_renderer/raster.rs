use crate::{
    color::{ColorTransform, IccProfile, IccRenderingIntent, RgbColorSpace},
    image::RasterPlane,
};

/// Converts ICC device samples at the renderer boundary. The returned plane
/// uses encoded sRGB coordinates and straight alpha for the existing upload
/// path; the source image remains in its original ICC representation.
pub(crate) fn convert_icc8_to_rgba8(
    width: u32,
    pixels: &[u8],
    profile: &IccProfile,
    intent: IccRenderingIntent,
    has_alpha: bool,
) -> Result<RasterPlane<u8>, Box<dyn std::error::Error>> {
    let to_pcs = profile.to_pcs(intent)?;
    let to_srgb = ColorTransform::pcs_to_rgb(profile.pcs(), RgbColorSpace::Srgb)?;
    let transfer = to_pcs.then(&to_srgb)?;
    let channels = profile.device_channels();
    let mut output = Vec::with_capacity(pixels.len() / (channels + usize::from(has_alpha)) * 4);
    for pixel in pixels.chunks_exact(channels + usize::from(has_alpha)) {
        let coordinates: Vec<_> = pixel[..channels]
            .iter()
            .map(|&sample| f64::from(sample) / 255.0)
            .collect();
        let rgb = transfer.transfer().evaluate(&coordinates)?;
        for &sample in &rgb[..3] {
            output.push((sample.clamp(0.0, 1.0) * 255.0).round() as u8);
        }
        output.push(if has_alpha { pixel[channels] } else { u8::MAX });
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
    fn rgb_expansion_discards_row_padding_and_supplies_opaque_alpha() {
        let plane = RasterPlane::new(9, vec![1, 2, 3, 4, 5, 6, 90, 91, 92]);

        let expanded = expand_rgb8_to_rgba8(2, 1, &plane);

        assert_eq!(expanded.row_stride(), 8);
        assert_eq!(expanded.samples(), &[1, 2, 3, 255, 4, 5, 6, 255]);
    }
}
