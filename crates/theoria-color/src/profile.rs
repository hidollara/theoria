use std::{
    error::Error,
    fmt,
    hash::{Hash, Hasher},
    sync::Arc,
};

use super::icc_profile::{IccProfileError, IccRenderingIntent};
use super::model::ColorModel;
use super::transfer::{Transfer, TransferError, TransferStage};

/// The coordinate representation used to connect color profiles.
///
/// Each variant denotes a different D50 color space, not one shared space.
/// Profile conversions expose actual XYZ or Lab coordinates here, rather than
/// the normalized encodings used inside some ICC LUT tags.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Pcs {
    XyzD50,
    LabD50,
}

/// A shared, runtime color space with its coordinate and conversion definition.
/// Clones retain the same identity; loading another definition creates another space.
#[derive(Clone)]
pub struct ColorSpace(Arc<dyn ColorProfile>);

impl ColorSpace {
    /// Owns a definition as a new color space. Named spaces use their canonical
    /// `Into<ColorSpace>` conversion instead.
    pub fn new(definition: impl ColorProfile + 'static) -> Self {
        assert!(
            definition.model().channels() > 0,
            "color model needs channels"
        );
        Self(Arc::new(definition))
    }

    /// Reuses an already shared definition without another allocation.
    pub fn from_arc<D: ColorProfile + 'static>(definition: Arc<D>) -> Self {
        assert!(
            definition.model().channels() > 0,
            "color model needs channels"
        );
        Self(definition)
    }

    pub fn channels(&self) -> usize {
        self.model().channels()
    }

    pub fn model(&self) -> ColorModel {
        self.0.model()
    }

    pub fn pcs(&self) -> Pcs {
        self.0.pcs()
    }

    /// Returns a raw directed transfer. Use [`connect_spaces`] to join two
    /// spaces with PCS and intent semantics checked for the whole route.
    pub fn space_to_pcs(
        &self,
        intent: IccRenderingIntent,
    ) -> Result<Transfer, ProfileConnectionError> {
        let transfer = self.0.space_to_pcs_transfer(intent)?;
        require_channels(self.channels(), transfer.input_channels())?;
        require_channels(3, transfer.output_channels())?;
        Ok(transfer)
    }

    /// Returns a raw directed transfer. It is not inferred by inverting the
    /// space-to-PCS direction.
    pub fn pcs_to_space(
        &self,
        intent: IccRenderingIntent,
    ) -> Result<Transfer, ProfileConnectionError> {
        let transfer = self.0.pcs_to_space_transfer(intent)?;
        require_channels(3, transfer.input_channels())?;
        require_channels(self.channels(), transfer.output_channels())?;
        Ok(transfer)
    }

    pub fn direct_to(
        &self,
        destination: &ColorSpace,
        intent: IccRenderingIntent,
    ) -> Result<Option<Transfer>, ProfileConnectionError> {
        self.0
            .direct_transfer_to(destination, intent)?
            .map(|transfer| {
                require_channels(self.channels(), transfer.input_channels())?;
                require_channels(destination.channels(), transfer.output_channels())?;
                Ok(transfer)
            })
            .transpose()
    }
}

fn require_channels(expected: usize, actual: usize) -> Result<(), ProfileConnectionError> {
    if expected == actual {
        Ok(())
    } else {
        Err(ProfileConnectionError::Channels { expected, actual })
    }
}

impl From<&ColorSpace> for ColorSpace {
    fn from(profile: &ColorSpace) -> Self {
        profile.clone()
    }
}

impl PartialEq for ColorSpace {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for ColorSpace {}

impl Hash for ColorSpace {
    fn hash<H: Hasher>(&self, state: &mut H) {
        (Arc::as_ptr(&self.0) as *const ()).hash(state);
    }
}

impl fmt::Debug for ColorSpace {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("ColorSpace")
            .field(&(Arc::as_ptr(&self.0) as *const ()))
            .finish()
    }
}

/// Coordinate model and directed conversions defining one runtime color space.
/// The two PCS directions are independent, and a direct link is optional.
/// Definitions are `Send + Sync` so their handles can be shared by renderers.
/// `model` and `pcs` must remain stable for the lifetime of the definition.
pub trait ColorProfile: Send + Sync {
    fn model(&self) -> ColorModel;
    fn pcs(&self) -> Pcs;
    fn space_to_pcs_transfer(
        &self,
        intent: IccRenderingIntent,
    ) -> Result<Transfer, ProfileConnectionError>;
    fn pcs_to_space_transfer(
        &self,
        intent: IccRenderingIntent,
    ) -> Result<Transfer, ProfileConnectionError>;

    /// A one-way conversion to a particular destination profile, if available.
    ///
    /// The destination profile and intent are part of a link's meaning. A link
    /// must not stand in for another merely because its space endpoints match.
    fn direct_transfer_to(
        &self,
        _destination: &ColorSpace,
        _intent: IccRenderingIntent,
    ) -> Result<Option<Transfer>, ProfileConnectionError> {
        Ok(None)
    }
}

/// Selects a direct link when the endpoints and intent match, otherwise joins
/// the source and destination profiles through their PCS representations.
/// Unlike [`Transfer::then`], this checks the semantic PCS and intent route.
pub fn connect_spaces(
    source: impl Into<ColorSpace>,
    destination: impl Into<ColorSpace>,
    source_intent: IccRenderingIntent,
    destination_intent: IccRenderingIntent,
) -> Result<Transfer, ProfileConnectionError> {
    let source = source.into();
    let destination = destination.into();
    if (source_intent == IccRenderingIntent::AbsoluteColorimetric)
        != (destination_intent == IccRenderingIntent::AbsoluteColorimetric)
    {
        return Err(ProfileConnectionError::IncompatibleIntentSpaces);
    }
    if source_intent == destination_intent
        && let Some(direct) = source.direct_to(&destination, source_intent)?
    {
        return Ok(direct);
    }
    let mut transform = source.space_to_pcs(source_intent)?;
    if source.pcs() != destination.pcs() {
        let bridge = match (source.pcs(), destination.pcs()) {
            (Pcs::XyzD50, Pcs::LabD50) => TransferStage::XyzD50ToLabD50,
            (Pcs::LabD50, Pcs::XyzD50) => TransferStage::LabD50ToXyzD50,
            _ => unreachable!("different PCS representations"),
        };
        transform = transform.then(&Transfer::new(3, [bridge])?)?;
    }
    let destination_transform = destination.pcs_to_space(destination_intent)?;
    Ok(transform.then(&destination_transform)?)
}

pub(crate) fn require_relative(intent: IccRenderingIntent) -> Result<(), ProfileConnectionError> {
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
    Channels { expected: usize, actual: usize },
    Transfer(TransferError),
    Icc(IccProfileError),
}

impl From<TransferError> for ProfileConnectionError {
    fn from(error: TransferError) -> Self {
        Self::Transfer(error)
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
            Self::UnsupportedIntent(intent) => {
                write!(formatter, "color space does not provide {intent:?} intent")
            }
            Self::Channels { expected, actual } => {
                write!(
                    formatter,
                    "color space requires {expected} channels, got {actual}"
                )
            }
            Self::Transfer(error) => error.fmt(formatter),
            Self::Icc(error) => error.fmt(formatter),
        }
    }
}

impl Error for ProfileConnectionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Transfer(error) => Some(error),
            Self::Icc(error) => Some(error),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::icc_profile::IccProfile;
    use crate::space::{DisplayP3, LabD50, LinearSrgb, Srgb};
    use crate::transfer::{Transfer, TransferCurve, TransferStage};
    use std::collections::HashSet;

    #[test]
    fn predefined_profiles_share_handles_while_other_definitions_are_distinct() {
        let canonical = ColorSpace::from(Srgb);
        let another_reference = ColorSpace::from(Srgb);
        let custom = IccProfile::from_bytes(&moxcms::ColorProfile::new_srgb().encode().unwrap())
            .unwrap()
            .color_space();
        assert_eq!(canonical, another_reference);
        assert_eq!(canonical.model(), ColorModel::Rgb);
        assert_ne!(canonical, custom);
        let handles = HashSet::from([canonical.clone(), another_reference, custom]);
        assert_eq!(handles.len(), 2);
    }

    #[test]
    fn known_encoding_link_is_selected_without_a_pcs_round_trip() {
        let transform = connect_spaces(
            Srgb,
            LinearSrgb,
            IccRenderingIntent::RelativeColorimetric,
            IccRenderingIntent::RelativeColorimetric,
        )
        .unwrap();
        assert_eq!(
            transform.stages(),
            &[TransferStage::curves([TransferCurve::SRGB_DECODE; 3]).unwrap()]
        );
        let expected = crate::transform::decode_srgb_f64(0.5);
        for actual in transform.evaluate(&[0.5; 3]).unwrap() {
            assert!((actual - expected).abs() < 1e-12);
        }
    }

    #[test]
    fn builtin_direct_link_does_not_target_a_loaded_icc_space() {
        let bytes = moxcms::ColorProfile::new_srgb().encode().unwrap();
        let icc = IccProfile::from_bytes(&bytes).unwrap();
        assert_ne!(icc.color_space(), Srgb.into());
        assert!(
            ColorSpace::from(LinearSrgb)
                .direct_to(&icc.color_space(), IccRenderingIntent::RelativeColorimetric)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn differing_primaries_fall_back_to_profile_pcs_directions() {
        let actual = connect_spaces(
            Srgb,
            DisplayP3,
            IccRenderingIntent::RelativeColorimetric,
            IccRenderingIntent::RelativeColorimetric,
        )
        .unwrap();
        let expected = ColorSpace::from(Srgb)
            .space_to_pcs(IccRenderingIntent::RelativeColorimetric)
            .unwrap()
            .then(
                &ColorSpace::from(DisplayP3)
                    .pcs_to_space(IccRenderingIntent::RelativeColorimetric)
                    .unwrap(),
            )
            .unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn icc_and_predefined_profiles_share_the_connection_path() {
        let bytes = moxcms::ColorProfile::new_srgb().encode().unwrap();
        let icc = IccProfile::from_bytes(&bytes).unwrap();
        let transform = connect_spaces(
            &icc,
            LinearSrgb,
            IccRenderingIntent::RelativeColorimetric,
            IccRenderingIntent::RelativeColorimetric,
        )
        .unwrap();
        let converted = transform.evaluate(&[0.5; 3]).unwrap();
        assert!(converted.iter().all(|value| *value > 0.2 && *value < 0.23));
    }

    #[test]
    fn predefined_and_icc_spaces_round_trip_through_pcs() {
        let bytes = moxcms::ColorProfile::new_srgb().encode().unwrap();
        let icc = IccProfile::from_bytes(&bytes).unwrap();
        let intent = IccRenderingIntent::RelativeColorimetric;
        let forward = connect_spaces(Srgb, &icc, intent, intent).unwrap();
        let backward = connect_spaces(&icc, Srgb, intent, intent).unwrap();
        let sample = [0.3, 0.5, 0.7];
        let converted = forward.evaluate(&sample).unwrap();
        let restored = backward.evaluate(&converted).unwrap();
        for (actual, expected) in restored.into_iter().zip(sample) {
            assert!((actual - expected).abs() < 0.004);
        }
    }

    #[test]
    fn profiles_can_be_connected_after_type_erasure() {
        let source: ColorSpace = Srgb.into();
        let destination: ColorSpace = LabD50.into();
        let transform = connect_spaces(
            &source,
            &destination,
            IccRenderingIntent::RelativeColorimetric,
            IccRenderingIntent::RelativeColorimetric,
        )
        .unwrap();
        assert_eq!(transform.evaluate(&[1.0; 3]).unwrap()[0], 100.0);
    }

    #[test]
    fn pcs_bridge_round_trips_between_rgb_and_lab() {
        let intent = IccRenderingIntent::RelativeColorimetric;
        let to_lab = connect_spaces(LinearSrgb, LabD50, intent, intent).unwrap();
        let to_rgb = connect_spaces(LabD50, LinearSrgb, intent, intent).unwrap();
        let sample = [0.2, 0.4, 0.6];
        let lab = to_lab.evaluate(&sample).unwrap();
        let restored = to_rgb.evaluate(&lab).unwrap();
        for (actual, expected) in restored.into_iter().zip(sample) {
            assert!((actual - expected).abs() < 1e-7);
        }
    }

    #[test]
    fn direct_link_is_used_only_for_its_declared_intent() {
        struct LinkedSpace;

        impl ColorProfile for LinkedSpace {
            fn model(&self) -> ColorModel {
                ColorModel::Rgb
            }

            fn pcs(&self) -> Pcs {
                Pcs::XyzD50
            }

            fn space_to_pcs_transfer(
                &self,
                _: IccRenderingIntent,
            ) -> Result<Transfer, ProfileConnectionError> {
                ColorSpace::from(Srgb).space_to_pcs(IccRenderingIntent::RelativeColorimetric)
            }

            fn pcs_to_space_transfer(
                &self,
                _: IccRenderingIntent,
            ) -> Result<Transfer, ProfileConnectionError> {
                unreachable!()
            }

            fn direct_transfer_to(
                &self,
                destination: &ColorSpace,
                intent: IccRenderingIntent,
            ) -> Result<Option<Transfer>, ProfileConnectionError> {
                if *destination == ColorSpace::from(LinearSrgb)
                    && intent == IccRenderingIntent::Saturation
                {
                    return Ok(Some(Transfer::new(3, Vec::<TransferStage>::new())?));
                }
                Ok(None)
            }
        }

        let direct = connect_spaces(
            ColorSpace::new(LinkedSpace),
            LinearSrgb,
            IccRenderingIntent::Saturation,
            IccRenderingIntent::Saturation,
        )
        .unwrap();
        assert!(direct.stages().is_empty());

        let fallback = connect_spaces(
            ColorSpace::new(LinkedSpace),
            LinearSrgb,
            IccRenderingIntent::Saturation,
            IccRenderingIntent::RelativeColorimetric,
        )
        .unwrap();
        assert!(!fallback.stages().is_empty());
    }

    #[test]
    fn unsupported_builtin_intent_is_not_misrepresented_as_relative() {
        assert!(matches!(
            connect_spaces(
                Srgb,
                LinearSrgb,
                IccRenderingIntent::Perceptual,
                IccRenderingIntent::Perceptual,
            ),
            Err(ProfileConnectionError::UnsupportedIntent(
                IccRenderingIntent::Perceptual
            ))
        ));
    }

    #[test]
    fn direct_transfer_with_wrong_channel_count_is_rejected() {
        struct InvalidLink;

        impl ColorProfile for InvalidLink {
            fn model(&self) -> ColorModel {
                ColorModel::Rgb
            }

            fn pcs(&self) -> Pcs {
                Pcs::XyzD50
            }

            fn space_to_pcs_transfer(
                &self,
                _: IccRenderingIntent,
            ) -> Result<Transfer, ProfileConnectionError> {
                unreachable!()
            }

            fn pcs_to_space_transfer(
                &self,
                _: IccRenderingIntent,
            ) -> Result<Transfer, ProfileConnectionError> {
                unreachable!()
            }

            fn direct_transfer_to(
                &self,
                _: &ColorSpace,
                _: IccRenderingIntent,
            ) -> Result<Option<Transfer>, ProfileConnectionError> {
                Ok(Some(Transfer::new(4, Vec::<TransferStage>::new())?))
            }
        }

        assert!(matches!(
            connect_spaces(
                ColorSpace::new(InvalidLink),
                LinearSrgb,
                IccRenderingIntent::RelativeColorimetric,
                IccRenderingIntent::RelativeColorimetric,
            ),
            Err(ProfileConnectionError::Channels { .. })
        ));
    }

    #[test]
    fn profile_transfer_with_wrong_channel_count_is_rejected() {
        struct InvalidPcs;

        impl ColorProfile for InvalidPcs {
            fn model(&self) -> ColorModel {
                ColorModel::Rgb
            }

            fn pcs(&self) -> Pcs {
                Pcs::XyzD50
            }

            fn space_to_pcs_transfer(
                &self,
                _: IccRenderingIntent,
            ) -> Result<Transfer, ProfileConnectionError> {
                Ok(Transfer::new(4, Vec::<TransferStage>::new())?)
            }

            fn pcs_to_space_transfer(
                &self,
                _: IccRenderingIntent,
            ) -> Result<Transfer, ProfileConnectionError> {
                unreachable!()
            }
        }

        assert!(matches!(
            connect_spaces(
                ColorSpace::new(InvalidPcs),
                LinearSrgb,
                IccRenderingIntent::RelativeColorimetric,
                IccRenderingIntent::RelativeColorimetric,
            ),
            Err(ProfileConnectionError::Channels { .. })
        ));
    }
}
