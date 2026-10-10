#[cfg(test)]
mod tests {
    use crate::reference::convert_icc8_to_linear_rgba16;
    use std::sync::Arc;
    use theoria_canvas::canvas::{
        Image,
        image::{ImageKind, RasterImageView},
    };
    use theoria_canvas::image::{AlphaAssociation, IccImageError, RasterImage, RasterPlane, Rgba8};
    use theoria_color::{DisplayP3, IccProfile, IccRenderingIntent, LinearSrgb, Srgb};

    #[test]
    fn typed_raster_image_retains_its_space_after_erasure() {
        let raster = RasterImage::<Rgba8, DisplayP3>::from_planes(
            1,
            1,
            vec![RasterPlane::new(4, vec![255, 0, 0, 255])],
            DisplayP3,
            AlphaAssociation::Straight,
        )
        .unwrap();
        let image = Image::from(raster);
        let ImageKind::Raster(source) = image.kind() else {
            panic!("typed raster image was not retained")
        };
        let RasterImageView::Rgba8 { color_space, .. } = source.view() else {
            panic!("typed raster image lost its format")
        };

        assert_eq!(color_space, DisplayP3.into());
    }

    #[test]
    fn icc_image_preserves_samples_until_renderer_conversion() {
        let profile = IccProfile::from_bytes(include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/crates/theoria-color/src/color/fixtures/sRGB-v4.icc"
        )))
        .unwrap();
        let source: Arc<[u8]> = vec![64, 128, 192, 77].into();
        let image = Image::from_icc_u8(
            1,
            1,
            source.clone(),
            profile,
            IccRenderingIntent::RelativeColorimetric,
            true,
        )
        .unwrap();
        let ImageKind::Raster(raster) = image.kind() else {
            panic!("ICC image was materialized at the Canvas boundary")
        };
        let RasterImageView::Icc8 {
            pixels,
            profile,
            intent,
            has_alpha,
        } = raster.view()
        else {
            panic!("ICC samples were replaced before renderer conversion")
        };
        assert_eq!(pixels, source.as_ref());
        let converted =
            convert_icc8_to_linear_rgba16(1, pixels, profile, intent, has_alpha).unwrap();
        let expected = theoria_color::ColorSpace::from(Srgb)
            .transfer_to(LinearSrgb)
            .unwrap()
            .evaluate(&[64.0 / 255.0, 128.0 / 255.0, 192.0 / 255.0])
            .unwrap();
        for (actual, expected) in converted.samples()[..3].iter().zip(expected) {
            assert!((half::f16::from_bits(*actual).to_f32() - expected as f32).abs() < 0.02);
        }
        assert!(
            (half::f16::from_bits(converted.samples()[3]).to_f32() - 77.0 / 255.0).abs() < 0.002
        );
    }

    #[test]
    fn icc_image_checks_sample_count_at_construction() {
        let profile = IccProfile::from_bytes(include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/crates/theoria-color/src/color/fixtures/sGrey-v4.icc"
        )))
        .unwrap();
        assert!(matches!(
            Image::from_icc_u8(
                2,
                1,
                vec![127],
                profile,
                IccRenderingIntent::RelativeColorimetric,
                false,
            ),
            Err(IccImageError::InvalidDataLength {
                expected: 2,
                actual: 1
            })
        ));
    }
}
