use std::{error::Error, fmt};

use super::color_transform::{ColorTransform, ColorTransformError};
use super::icc::IccPcs;
use super::icc_profile::{IccProfile, IccProfileError, IccRenderingIntent};
use super::profile::{
    ColorSpaceKey, DisplayP3, LabD50, LinearDisplayP3, LinearSrgb, RgbColorSpace, Srgb, XyzD50,
    XyzD65,
};
use super::profile_data::{XYZ_D50_TO_XYZ_D65, XYZ_D65_TO_XYZ_D50};
use super::transfer::{Transfer, TransferError, TransferMatrix, TransferStage};

/// Directed conversions supplied by a profile for one coordinate space.
///
/// The space identity is separate from its conversion data: two profiles can
/// interpret the same number of channels differently. The two PCS directions
/// are independent, and a direct link is optional rather than an inverse of
/// either direction.
pub trait ColorProfile {
    fn space(&self) -> ColorSpaceKey;
    fn pcs(&self) -> IccPcs;
    fn to_pcs(&self, intent: IccRenderingIntent) -> Result<ColorTransform, ProfileConnectionError>;
    fn from_pcs(
        &self,
        intent: IccRenderingIntent,
    ) -> Result<ColorTransform, ProfileConnectionError>;

    /// A one-way conversion to a particular coordinate space, if available.
    ///
    /// The intent is part of a link's meaning. A link for one intent must not
    /// silently stand in for another, even if its endpoints happen to match.
    fn direct_to(
        &self,
        _destination: &ColorSpaceKey,
        _intent: IccRenderingIntent,
    ) -> Result<Option<ColorTransform>, ProfileConnectionError> {
        Ok(None)
    }
}

/// Selects a direct link when the endpoints and intent match, otherwise joins
/// the source and destination profiles through their PCS representations.
pub fn connect_profiles(
    source: &dyn ColorProfile,
    destination: &dyn ColorProfile,
    source_intent: IccRenderingIntent,
    destination_intent: IccRenderingIntent,
) -> Result<ColorTransform, ProfileConnectionError> {
    if (source_intent == IccRenderingIntent::AbsoluteColorimetric)
        != (destination_intent == IccRenderingIntent::AbsoluteColorimetric)
    {
        return Err(ProfileConnectionError::IncompatibleIntentSpaces);
    }
    let destination_space = destination.space();
    if source_intent == destination_intent {
        if let Some(direct) = source.direct_to(&destination_space, source_intent)? {
            if direct.from() != &source.space() || direct.to() != &destination_space {
                return Err(ProfileConnectionError::InvalidDirectLink);
            }
            return Ok(direct);
        }
    }
    let mut transform = source.to_pcs(source_intent)?;
    if source.pcs() != destination.pcs() {
        transform = transform.then(&ColorTransform::between_pcs_for_intent(
            source.pcs(),
            destination.pcs(),
            source_intent,
        )?)?;
    }
    Ok(transform.then(&destination.from_pcs(destination_intent)?)?)
}

impl ColorProfile for RgbColorSpace {
    fn space(&self) -> ColorSpaceKey {
        (*self).into()
    }

    fn pcs(&self) -> IccPcs {
        IccPcs::XyzD50
    }

    fn to_pcs(&self, intent: IccRenderingIntent) -> Result<ColorTransform, ProfileConnectionError> {
        require_relative(intent)?;
        Ok(ColorTransform::rgb_to_pcs(*self, self.pcs())?)
    }

    fn from_pcs(
        &self,
        intent: IccRenderingIntent,
    ) -> Result<ColorTransform, ProfileConnectionError> {
        require_relative(intent)?;
        Ok(ColorTransform::pcs_to_rgb(self.pcs(), *self)?)
    }

    fn direct_to(
        &self,
        destination: &ColorSpaceKey,
        intent: IccRenderingIntent,
    ) -> Result<Option<ColorTransform>, ProfileConnectionError> {
        if intent != IccRenderingIntent::RelativeColorimetric {
            return Ok(None);
        }
        for other in [
            RgbColorSpace::Srgb,
            RgbColorSpace::LinearSrgb,
            RgbColorSpace::DisplayP3,
            RgbColorSpace::LinearDisplayP3,
        ] {
            let same_primaries = matches!(
                (*self, other),
                (
                    RgbColorSpace::Srgb | RgbColorSpace::LinearSrgb,
                    RgbColorSpace::Srgb | RgbColorSpace::LinearSrgb
                ) | (
                    RgbColorSpace::DisplayP3 | RgbColorSpace::LinearDisplayP3,
                    RgbColorSpace::DisplayP3 | RgbColorSpace::LinearDisplayP3
                )
            );
            if same_primaries && ColorSpaceKey::from(other) == *destination {
                return Ok(Some(ColorTransform::between_rgb(*self, other)?));
            }
        }
        Ok(None)
    }
}

macro_rules! predefined_rgb_profile {
    ($($profile:ty),+ $(,)?) => {
        $(
            impl ColorProfile for $profile {
                fn space(&self) -> ColorSpaceKey {
                    (*self).into()
                }

                fn pcs(&self) -> IccPcs {
                    IccPcs::XyzD50
                }

                fn to_pcs(
                    &self,
                    intent: IccRenderingIntent,
                ) -> Result<ColorTransform, ProfileConnectionError> {
                    RgbColorSpace::from(*self).to_pcs(intent)
                }

                fn from_pcs(
                    &self,
                    intent: IccRenderingIntent,
                ) -> Result<ColorTransform, ProfileConnectionError> {
                    RgbColorSpace::from(*self).from_pcs(intent)
                }

                fn direct_to(
                    &self,
                    destination: &ColorSpaceKey,
                    intent: IccRenderingIntent,
                ) -> Result<Option<ColorTransform>, ProfileConnectionError> {
                    RgbColorSpace::from(*self).direct_to(destination, intent)
                }
            }
        )+
    };
}

predefined_rgb_profile!(Srgb, LinearSrgb, DisplayP3, LinearDisplayP3);

impl ColorProfile for XyzD50 {
    fn space(&self) -> ColorSpaceKey {
        (*self).into()
    }

    fn pcs(&self) -> IccPcs {
        IccPcs::XyzD50
    }

    fn to_pcs(&self, intent: IccRenderingIntent) -> Result<ColorTransform, ProfileConnectionError> {
        require_relative(intent)?;
        Ok(ColorTransform::between_pcs(self.pcs(), self.pcs())?)
    }

    fn from_pcs(
        &self,
        intent: IccRenderingIntent,
    ) -> Result<ColorTransform, ProfileConnectionError> {
        self.to_pcs(intent)
    }
}

impl ColorProfile for LabD50 {
    fn space(&self) -> ColorSpaceKey {
        (*self).into()
    }

    fn pcs(&self) -> IccPcs {
        IccPcs::LabD50
    }

    fn to_pcs(&self, intent: IccRenderingIntent) -> Result<ColorTransform, ProfileConnectionError> {
        require_relative(intent)?;
        Ok(ColorTransform::between_pcs(self.pcs(), self.pcs())?)
    }

    fn from_pcs(
        &self,
        intent: IccRenderingIntent,
    ) -> Result<ColorTransform, ProfileConnectionError> {
        self.to_pcs(intent)
    }
}

impl ColorProfile for XyzD65 {
    fn space(&self) -> ColorSpaceKey {
        (*self).into()
    }

    fn pcs(&self) -> IccPcs {
        IccPcs::XyzD50
    }

    fn to_pcs(&self, intent: IccRenderingIntent) -> Result<ColorTransform, ProfileConnectionError> {
        require_relative(intent)?;
        Ok(ColorTransform::new(
            self.space(),
            XyzD50.into(),
            adaptation(XYZ_D65_TO_XYZ_D50)?,
        )?)
    }

    fn from_pcs(
        &self,
        intent: IccRenderingIntent,
    ) -> Result<ColorTransform, ProfileConnectionError> {
        require_relative(intent)?;
        Ok(ColorTransform::new(
            XyzD50.into(),
            self.space(),
            adaptation(XYZ_D50_TO_XYZ_D65)?,
        )?)
    }
}

fn adaptation(matrix: [[f64; 3]; 3]) -> Result<Transfer, ProfileConnectionError> {
    Ok(Transfer::new(
        3,
        [TransferStage::Matrix(TransferMatrix::new(
            3,
            3,
            matrix.into_iter().flatten().collect::<Vec<_>>(),
            [0.0; 3],
        )?)],
    )?)
}

impl ColorProfile for IccProfile {
    fn space(&self) -> ColorSpaceKey {
        self.device_space()
    }

    fn pcs(&self) -> IccPcs {
        self.pcs()
    }

    fn to_pcs(&self, intent: IccRenderingIntent) -> Result<ColorTransform, ProfileConnectionError> {
        Ok(IccProfile::to_pcs(self, intent)?)
    }

    fn from_pcs(
        &self,
        intent: IccRenderingIntent,
    ) -> Result<ColorTransform, ProfileConnectionError> {
        Ok(IccProfile::from_pcs(self, intent)?)
    }
}

fn require_relative(intent: IccRenderingIntent) -> Result<(), ProfileConnectionError> {
    if intent == IccRenderingIntent::RelativeColorimetric {
        Ok(())
    } else {
        Err(ProfileConnectionError::UnsupportedIntent(intent))
    }
}

#[derive(Debug)]
pub enum ProfileConnectionError {
    IncompatibleIntentSpaces,
    UnsupportedIntent(IccRenderingIntent),
    InvalidDirectLink,
    Transform(ColorTransformError),
    Icc(IccProfileError),
}

impl From<ColorTransformError> for ProfileConnectionError {
    fn from(error: ColorTransformError) -> Self {
        Self::Transform(error)
    }
}

impl From<TransferError> for ProfileConnectionError {
    fn from(error: TransferError) -> Self {
        Self::Transform(ColorTransformError::Transfer(error))
    }
}

impl From<IccProfileError> for ProfileConnectionError {
    fn from(error: IccProfileError) -> Self {
        Self::Icc(error)
    }
}

impl fmt::Display for ProfileConnectionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::IncompatibleIntentSpaces => {
                formatter.write_str("absolute and media-relative PCS cannot be connected")
            }
            Self::UnsupportedIntent(intent) => write!(
                formatter,
                "predefined RGB profile does not provide {intent:?} intent"
            ),
            Self::InvalidDirectLink => {
                formatter.write_str("direct profile link has incorrect coordinate-space endpoints")
            }
            Self::Transform(error) => error.fmt(formatter),
            Self::Icc(error) => error.fmt(formatter),
        }
    }
}

impl Error for ProfileConnectionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Transform(error) => Some(error),
            Self::Icc(error) => Some(error),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transfer::{Transfer, TransferCurve, TransferStage};

    #[test]
    fn known_encoding_link_is_selected_without_a_pcs_round_trip() {
        let transform = connect_profiles(
            &RgbColorSpace::Srgb,
            &RgbColorSpace::LinearSrgb,
            IccRenderingIntent::RelativeColorimetric,
            IccRenderingIntent::RelativeColorimetric,
        )
        .unwrap();
        assert_eq!(
            transform.transfer().stages(),
            &[TransferStage::curves([TransferCurve::SRGB_DECODE; 3]).unwrap()]
        );
        assert_eq!(transform.from(), &ColorSpaceKey::from(RgbColorSpace::Srgb));
        assert_eq!(
            transform.to(),
            &ColorSpaceKey::from(RgbColorSpace::LinearSrgb)
        );
    }

    #[test]
    fn differing_primaries_fall_back_to_profile_pcs_directions() {
        let actual = connect_profiles(
            &RgbColorSpace::Srgb,
            &RgbColorSpace::DisplayP3,
            IccRenderingIntent::RelativeColorimetric,
            IccRenderingIntent::RelativeColorimetric,
        )
        .unwrap();
        let expected = RgbColorSpace::Srgb
            .to_pcs(IccRenderingIntent::RelativeColorimetric)
            .unwrap()
            .then(
                &RgbColorSpace::DisplayP3
                    .from_pcs(IccRenderingIntent::RelativeColorimetric)
                    .unwrap(),
            )
            .unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn icc_and_predefined_profiles_share_the_connection_path() {
        let bytes = moxcms::ColorProfile::new_srgb().encode().unwrap();
        let icc = IccProfile::from_bytes(&bytes).unwrap();
        let transform = connect_profiles(
            &icc,
            &RgbColorSpace::LinearSrgb,
            IccRenderingIntent::RelativeColorimetric,
            IccRenderingIntent::RelativeColorimetric,
        )
        .unwrap();
        assert_eq!(transform.from(), &icc.device_space());
        assert_eq!(
            transform.to(),
            &ColorSpaceKey::from(RgbColorSpace::LinearSrgb)
        );
        let converted = transform.transfer().evaluate(&[0.5; 3]).unwrap();
        assert!(converted.iter().all(|value| *value > 0.2 && *value < 0.23));
    }

    #[test]
    fn unsupported_builtin_intent_is_not_misrepresented_as_relative() {
        assert!(matches!(
            connect_profiles(
                &RgbColorSpace::Srgb,
                &RgbColorSpace::LinearSrgb,
                IccRenderingIntent::Perceptual,
                IccRenderingIntent::Perceptual,
            ),
            Err(ProfileConnectionError::UnsupportedIntent(
                IccRenderingIntent::Perceptual
            ))
        ));
    }

    #[test]
    fn malformed_direct_link_cannot_escape_with_a_matching_channel_count() {
        struct InvalidLink;

        impl ColorProfile for InvalidLink {
            fn space(&self) -> ColorSpaceKey {
                RgbColorSpace::Srgb.into()
            }

            fn pcs(&self) -> IccPcs {
                IccPcs::XyzD50
            }

            fn to_pcs(
                &self,
                _: IccRenderingIntent,
            ) -> Result<ColorTransform, ProfileConnectionError> {
                unreachable!()
            }

            fn from_pcs(
                &self,
                _: IccRenderingIntent,
            ) -> Result<ColorTransform, ProfileConnectionError> {
                unreachable!()
            }

            fn direct_to(
                &self,
                _: &ColorSpaceKey,
                _: IccRenderingIntent,
            ) -> Result<Option<ColorTransform>, ProfileConnectionError> {
                Ok(Some(ColorTransform::new(
                    RgbColorSpace::DisplayP3.into(),
                    RgbColorSpace::LinearSrgb.into(),
                    Transfer::new(3, Vec::<TransferStage>::new()).unwrap(),
                )?))
            }
        }

        assert!(matches!(
            connect_profiles(
                &InvalidLink,
                &RgbColorSpace::LinearSrgb,
                IccRenderingIntent::RelativeColorimetric,
                IccRenderingIntent::RelativeColorimetric,
            ),
            Err(ProfileConnectionError::InvalidDirectLink)
        ));
    }
}
