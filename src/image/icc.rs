use std::{error::Error, fmt, sync::Arc};

use crate::color::{ColorTransform, IccProfile, IccRenderingIntent, RgbColorSpace};

use super::{RasterImageInstanceKey, Srgba8Image};

/// Interleaved normalized `u8` samples with their original ICC meaning.
///
/// The profile's device-channel count determines the color channels. An
/// optional final byte stores straight alpha independently of the profile.
#[derive(Clone, Debug)]
pub struct IccRasterImage {
    pub(crate) owner: Arc<()>,
    width: u32,
    height: u32,
    pixels: Arc<[u8]>,
    profile: IccProfile,
    intent: IccRenderingIntent,
    has_alpha: bool,
}

impl IccRasterImage {
    /// Encodes a CPU-readable sRGBA8 image into a destination ICC profile.
    /// This is a renderer/output-boundary operation; Canvas declarations keep
    /// the original color coordinates and never perform this conversion.
    pub fn from_srgba8(
        image: &Srgba8Image,
        profile: IccProfile,
        intent: IccRenderingIntent,
    ) -> Result<Self, Box<dyn Error>> {
        if intent == IccRenderingIntent::AbsoluteColorimetric {
            return Err(Box::new(IccImageError::AbsoluteIntent));
        }
        let from_rgb = ColorTransform::rgb_to_pcs(RgbColorSpace::Srgb, profile.pcs())?;
        let to_device = profile.from_pcs(intent)?;
        let transform = from_rgb.then(&to_device)?;
        let channels = profile.device_channels();
        let mut pixels = Vec::with_capacity(
            image
                .pixels()
                .len()
                .checked_div(4)
                .and_then(|count| count.checked_mul(channels + 1))
                .ok_or(IccImageError::DimensionsTooLarge)?,
        );
        for rgba in image.pixels().chunks_exact(4) {
            let rgb = rgba[..3]
                .iter()
                .map(|&sample| f64::from(sample) / 255.0)
                .collect::<Vec<_>>();
            let device = transform.transfer().evaluate(&rgb)?;
            for value in device {
                pixels.push((value.clamp(0.0, 1.0) * 255.0).round() as u8);
            }
            pixels.push(rgba[3]);
        }
        Ok(Self::new(
            image.width(),
            image.height(),
            pixels,
            profile,
            intent,
            true,
        )?)
    }

    pub fn new(
        width: u32,
        height: u32,
        pixels: impl Into<Arc<[u8]>>,
        profile: IccProfile,
        intent: IccRenderingIntent,
        has_alpha: bool,
    ) -> Result<Self, IccImageError> {
        if width == 0 || height == 0 {
            return Err(IccImageError::Empty);
        }
        if intent == IccRenderingIntent::AbsoluteColorimetric {
            return Err(IccImageError::AbsoluteIntent);
        }
        let pixels = pixels.into();
        let channels = profile.device_channels() + usize::from(has_alpha);
        let expected = usize::try_from(width)
            .ok()
            .and_then(|width| width.checked_mul(usize::try_from(height).ok()?))
            .and_then(|pixels| pixels.checked_mul(channels))
            .ok_or(IccImageError::DimensionsTooLarge)?;
        if pixels.len() != expected {
            return Err(IccImageError::InvalidDataLength {
                expected,
                actual: pixels.len(),
            });
        }
        Ok(Self {
            owner: Arc::new(()),
            width,
            height,
            pixels,
            profile,
            intent,
            has_alpha,
        })
    }

    pub const fn width(&self) -> u32 {
        self.width
    }

    pub const fn height(&self) -> u32 {
        self.height
    }

    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    pub const fn profile(&self) -> &IccProfile {
        &self.profile
    }

    pub const fn intent(&self) -> IccRenderingIntent {
        self.intent
    }

    pub const fn has_alpha(&self) -> bool {
        self.has_alpha
    }

    pub(crate) fn instance_key(&self) -> RasterImageInstanceKey {
        RasterImageInstanceKey::from_owner(&self.owner)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IccImageError {
    Empty,
    DimensionsTooLarge,
    InvalidDataLength { expected: usize, actual: usize },
    AbsoluteIntent,
}

impl fmt::Display for IccImageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("ICC image dimensions must be non-zero"),
            Self::DimensionsTooLarge => formatter.write_str("ICC image dimensions are too large"),
            Self::InvalidDataLength { expected, actual } => {
                write!(formatter, "ICC image needs {expected} bytes, got {actual}")
            }
            Self::AbsoluteIntent => formatter
                .write_str("absolute ICC image conversion needs an explicit destination profile"),
        }
    }
}

impl Error for IccImageError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas_renderer::raster::convert_icc8_to_rgba8;

    #[test]
    fn real_profile_output_can_be_sampled_as_a_canvas_image() {
        let source = Srgba8Image::new(2, 1, vec![64, 128, 192, 77, 200, 20, 40, 255]).unwrap();
        let profile =
            IccProfile::from_bytes(include_bytes!("../color/fixtures/DisplayP3-v4.icc")).unwrap();
        let encoded =
            IccRasterImage::from_srgba8(&source, profile, IccRenderingIntent::RelativeColorimetric)
                .unwrap();
        assert_eq!(encoded.pixels().len(), 8);
        assert_eq!(encoded.pixels()[3], 77);
        assert_eq!(encoded.pixels()[7], 255);
        let restored = convert_icc8_to_rgba8(
            2,
            encoded.pixels(),
            encoded.profile(),
            encoded.intent(),
            encoded.has_alpha(),
        )
        .unwrap();
        for (actual, expected) in restored.samples().iter().zip(source.pixels()) {
            assert!(actual.abs_diff(*expected) <= 3, "{actual} != {expected}");
        }
        let canvas_image = crate::canvas::Image::from(encoded);
        assert_eq!((canvas_image.width(), canvas_image.height()), (2, 1));
    }

    #[test]
    fn output_profile_controls_channel_count() {
        let source = Srgba8Image::new(1, 1, vec![128, 128, 128, 255]).unwrap();
        let profile =
            IccProfile::from_bytes(include_bytes!("../color/fixtures/sGrey-v4.icc")).unwrap();
        let encoded =
            IccRasterImage::from_srgba8(&source, profile, IccRenderingIntent::RelativeColorimetric)
                .unwrap();
        assert_eq!(encoded.pixels().len(), 2);
        assert_eq!(encoded.pixels()[1], 255);
    }

    #[test]
    fn real_cmyk_profile_can_enter_the_canvas_image_pipeline() {
        let profile = IccProfile::from_bytes(include_bytes!(
            "../color/fixtures/CGATS001Compat-v2-micro.icc"
        ))
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
        let converted = convert_icc8_to_rgba8(
            1,
            image.pixels(),
            image.profile(),
            image.intent(),
            image.has_alpha(),
        )
        .unwrap();
        assert_eq!(converted.samples().len(), 4);
        assert_eq!(converted.samples()[3], 200);
        assert!(converted.samples()[..3].iter().any(|&value| value != 0));
    }
}
