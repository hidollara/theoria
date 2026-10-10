use std::io;

use theoria_canvas::image::RasterPlane;

pub struct PackedIccSamples {
    pub samples: Vec<u8>,
    pub layers: u32,
    pub row_stride: usize,
}

/// Packs unchanged ICC device samples and optional straight alpha into RGBA8
/// texture-array layers. Unused components are padding, not color conversion.
pub fn pack_icc_samples(
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

/// Expands an RGB8 plane into the portable RGBA8 upload representation used
/// by renderers whose sampled texture API has no three-channel byte format.
pub fn expand_rgb8_to_rgba8(width: u32, height: u32, plane: &RasterPlane<u8>) -> RasterPlane<u8> {
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
    fn rgb_expansion_discards_row_padding_and_supplies_opaque_alpha() {
        let plane = RasterPlane::new(9, vec![1, 2, 3, 4, 5, 6, 90, 91, 92]);

        let expanded = expand_rgb8_to_rgba8(2, 1, &plane);

        assert_eq!(expanded.row_stride(), 8);
        assert_eq!(expanded.samples(), &[1, 2, 3, 255, 4, 5, 6, 255]);
    }
}
