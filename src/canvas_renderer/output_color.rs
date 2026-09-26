use std::{error::Error, fmt};

use crate::color::{ColorTransform, IccProfile, IccRenderingIntent, RgbColorSpace, TransferError};

/// Conversion from the renderer's premultiplied RGB working image to
/// straight destination-profile device coordinates at the output boundary.
pub(crate) struct OutputColorTransform {
    transfer: ColorTransform,
}

#[derive(Debug)]
pub(crate) enum OutputColorError {
    UnsupportedChannels(usize),
    AbsoluteIntent,
    Transform(Box<dyn Error>),
}

impl fmt::Display for OutputColorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedChannels(channels) => write!(
                formatter,
                "GPU output targets need three ICC color channels, got {channels}"
            ),
            Self::AbsoluteIntent => formatter
                .write_str("absolute output intent requires an explicit display-white adaptation"),
            Self::Transform(error) => error.fmt(formatter),
        }
    }
}

impl Error for OutputColorError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Transform(error) => Some(error.as_ref()),
            _ => None,
        }
    }
}

impl OutputColorTransform {
    pub(crate) fn new(
        profile: &IccProfile,
        intent: IccRenderingIntent,
        working_space: RgbColorSpace,
    ) -> Result<Self, OutputColorError> {
        if profile.device_channels() != 3 {
            return Err(OutputColorError::UnsupportedChannels(
                profile.device_channels(),
            ));
        }
        if intent == IccRenderingIntent::AbsoluteColorimetric {
            return Err(OutputColorError::AbsoluteIntent);
        }
        let source = ColorTransform::rgb_to_pcs(working_space, profile.pcs())
            .map_err(|error| OutputColorError::Transform(Box::new(error)))?;
        let destination = profile
            .from_pcs(intent)
            .map_err(|error| OutputColorError::Transform(Box::new(error)))?;
        let transfer = source
            .then(&destination)
            .map_err(|error| OutputColorError::Transform(Box::new(error)))?;
        Ok(Self { transfer })
    }

    pub(crate) fn convert_premultiplied(&self, rgba: [f32; 4]) -> Result<[f32; 4], TransferError> {
        let alpha = rgba[3];
        if alpha == 0.0 {
            return Ok([0.0; 4]);
        }
        let rgb: Vec<_> = rgba[..3]
            .iter()
            .map(|&component| f64::from(component / alpha))
            .collect();
        let transformed = self.transfer.transfer().evaluate(&rgb)?;
        Ok([
            transformed[0] as f32 * alpha,
            transformed[1] as f32 * alpha,
            transformed[2] as f32 * alpha,
            alpha,
        ])
    }

    pub(crate) fn wgsl(&self) -> Result<String, TransferError> {
        let mut source = self.transfer.transfer().wgsl(0)?;
        source.push_str(include_str!(
            "../../shaders/canvas_renderer/icc_present.wgsl"
        ));
        Ok(source)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_display_profile_output_compiles_as_a_fragment_shader() {
        let profile = IccProfile::from_bytes(include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/crates/theoria-color/src/color/fixtures/DisplayP3-v4.icc"
        )))
        .unwrap();
        let output = OutputColorTransform::new(
            &profile,
            IccRenderingIntent::RelativeColorimetric,
            RgbColorSpace::LinearSrgb,
        )
        .unwrap();
        let converted = output.convert_premultiplied([0.1, 0.2, 0.3, 0.5]).unwrap();
        assert_eq!(converted[3], 0.5);
        let shader = output.wgsl().unwrap();
        let module = naga::front::wgsl::parse_str(&shader)
            .unwrap_or_else(|error| panic!("{}", error.emit_to_string(&shader)));
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap();
    }

    #[test]
    fn gpu_output_rejects_a_non_rgb_profile() {
        let profile = IccProfile::from_bytes(include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/crates/theoria-color/src/color/fixtures/sGrey-v4.icc"
        )))
        .unwrap();
        assert!(matches!(
            OutputColorTransform::new(
                &profile,
                IccRenderingIntent::RelativeColorimetric,
                RgbColorSpace::LinearSrgb
            ),
            Err(OutputColorError::UnsupportedChannels(1))
        ));
    }

    #[test]
    fn output_transfer_starts_in_the_selected_working_space() {
        let profile = IccProfile::from_bytes(include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/crates/theoria-color/src/color/fixtures/DisplayP3-v4.icc"
        )))
        .unwrap();
        let linear = OutputColorTransform::new(
            &profile,
            IccRenderingIntent::RelativeColorimetric,
            RgbColorSpace::LinearSrgb,
        )
        .unwrap();
        let encoded = OutputColorTransform::new(
            &profile,
            IccRenderingIntent::RelativeColorimetric,
            RgbColorSpace::Srgb,
        )
        .unwrap();
        let from_linear = linear
            .convert_premultiplied([0.214_041_14, 0.0, 0.0, 1.0])
            .unwrap();
        let from_encoded = encoded.convert_premultiplied([0.5, 0.0, 0.0, 1.0]).unwrap();
        for (actual, expected) in from_encoded.into_iter().zip(from_linear) {
            assert!((actual - expected).abs() < 0.000_01);
        }
    }
}
