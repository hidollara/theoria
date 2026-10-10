#[cfg(test)]
mod tests {
    use crate::reference::convert_icc8_to_linear_rgba16;
    use theoria_canvas::image::{IccRasterImage, Srgba8Image};
    use theoria_color::{ColorSpace, IccProfile, IccRenderingIntent, Srgb};

    #[test]
    fn real_profile_output_can_be_sampled_as_a_canvas_image() {
        let source = Srgba8Image::new(2, 1, vec![64, 128, 192, 77, 200, 20, 40, 255]).unwrap();
        let profile = IccProfile::from_bytes(include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/crates/theoria-color/src/color/fixtures/DisplayP3-v4.icc"
        )))
        .unwrap();
        let encoded =
            IccRasterImage::from_srgba8(&source, profile, IccRenderingIntent::RelativeColorimetric)
                .unwrap();
        assert_eq!(encoded.pixels().len(), 8);
        assert_eq!(encoded.pixels()[3], 77);
        assert_eq!(encoded.pixels()[7], 255);
        let restored = convert_icc8_to_linear_rgba16(
            2,
            encoded.pixels(),
            encoded.profile(),
            encoded.intent(),
            encoded.has_alpha(),
        )
        .unwrap();
        for (actual, expected) in restored
            .samples()
            .chunks_exact(4)
            .zip(source.pixels().chunks_exact(4))
        {
            let source_rgb: [f32; 3] =
                std::array::from_fn(|index| f32::from(expected[index]) / 255.0);
            let linear = ColorSpace::from(Srgb)
                .transfer_to(theoria_color::LinearSrgb)
                .unwrap()
                .evaluate(&source_rgb.map(f64::from))
                .unwrap();
            for (actual, expected) in actual[..3].iter().zip(linear) {
                assert!((half::f16::from_bits(*actual).to_f32() - expected as f32).abs() < 0.03);
            }
            assert!(
                (half::f16::from_bits(actual[3]).to_f32() - f32::from(expected[3]) / 255.0).abs()
                    < 0.002
            );
        }
        let canvas_image = theoria_canvas::canvas::Image::from(encoded);
        assert_eq!((canvas_image.width(), canvas_image.height()), (2, 1));
    }

    #[test]
    fn output_profile_controls_channel_count() {
        let source = Srgba8Image::new(1, 1, vec![128, 128, 128, 255]).unwrap();
        let profile = IccProfile::from_bytes(include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/crates/theoria-color/src/color/fixtures/sGrey-v4.icc"
        )))
        .unwrap();
        let encoded =
            IccRasterImage::from_srgba8(&source, profile, IccRenderingIntent::RelativeColorimetric)
                .unwrap();
        assert_eq!(encoded.pixels().len(), 2);
        assert_eq!(encoded.pixels()[1], 255);
    }

    #[test]
    fn real_cmyk_profile_can_enter_the_canvas_image_pipeline() {
        let profile = IccProfile::from_bytes(include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/crates/theoria-color/src/color/fixtures/CGATS001Compat-v2-micro.icc"
        )))
        .unwrap();
        assert_eq!(profile.device_channels(), 4);
        let image = IccRasterImage::new(
            1,
            1,
            vec![32, 64, 96, 128, 200],
            profile,
            IccRenderingIntent::Perceptual,
            true,
        )
        .unwrap();
        let converted = convert_icc8_to_linear_rgba16(
            1,
            image.pixels(),
            image.profile(),
            image.intent(),
            image.has_alpha(),
        )
        .unwrap();
        assert_eq!(converted.samples().len(), 4);
        assert!(
            (half::f16::from_bits(converted.samples()[3]).to_f32() - 200.0 / 255.0).abs() < 0.002
        );
        assert!(converted.samples()[..3].iter().any(|&value| value != 0));
    }
}
