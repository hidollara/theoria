use crate::image::RasterPlane;

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
