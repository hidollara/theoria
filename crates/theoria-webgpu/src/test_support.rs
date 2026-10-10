use std::io;

use half::f16;
use theoria_canvas::image::RasterPlane;
use theoria_color::{
    ColorProfile, ColorSpace, ConnectionSpace, IccProfile, IccRenderingIntent, LinearSrgb,
    SpaceConnectionError, Transfer,
};

/// A separate runtime space with the same conversions and coordinate geometry.
/// This exercises space identity without manufacturing another standard space.
pub(crate) fn distinct_color_space(space: impl Into<ColorSpace>) -> ColorSpace {
    struct Distinct(ColorSpace, u64);

    impl ColorProfile for Distinct {
        fn equivalent(&self, other: &dyn ColorProfile) -> bool {
            (other as &dyn std::any::Any)
                .downcast_ref::<Self>()
                .is_some_and(|other| self.1 == other.1)
        }

        fn hash(&self, mut state: &mut dyn std::hash::Hasher) {
            std::hash::Hash::hash(&self.1, &mut state);
        }

        fn model(&self) -> theoria_color::ColorModel {
            self.0.model()
        }

        fn connection_space(&self) -> ConnectionSpace {
            self.0.connection_space()
        }

        fn transfer_to_connection_space(&self) -> Result<Transfer, SpaceConnectionError> {
            self.0.transfer_to_connection_space()
        }

        fn transfer_from_connection_space(&self) -> Result<Transfer, SpaceConnectionError> {
            self.0.transfer_from_connection_space()
        }

        fn direct_transfer_to(
            &self,
            destination: &ColorSpace,
        ) -> Result<Option<Transfer>, SpaceConnectionError> {
            self.0.direct_to(destination)
        }
    }

    let space = space.into();
    static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    ColorSpace::new(Distinct(
        space,
        NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
    ))
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
