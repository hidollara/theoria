use std::{error::Error, fmt, sync::Arc};

use crate::color::{ColorSpace, IccProfile, IccRenderingIntent, Srgb};

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
        let transfer = ColorSpace::from(Srgb).transfer_to(profile.color_space(intent))?;
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
            let device = transfer.evaluate(&rgb)?;
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
