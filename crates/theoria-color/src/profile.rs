use std::{
    error::Error,
    fmt,
    hash::{Hash, Hasher},
    ops::Deref,
    sync::Arc,
};

use super::color_transform::{ColorTransform, ColorTransformError, pcs_key_for_intent};
use super::icc_profile::{IccProfileError, IccRenderingIntent};
use super::space::ColorSpace;
use super::transfer::TransferError;

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

/// A shared profile and the identity of its conversion definition.
///
/// Clones retain the same definition. Two handles can describe the same color
/// space while using different conversion data.
#[derive(Clone)]
pub struct ColorProfile(Arc<dyn ProfileDefinition>);

impl ColorProfile {
    /// Owns a definition as a new profile. Each call creates a distinct handle.
    pub fn new(definition: impl ProfileDefinition + 'static) -> Self {
        Self(Arc::new(definition))
    }

    /// Reuses an already shared definition without another allocation.
    pub fn from_arc<D: ProfileDefinition + 'static>(definition: Arc<D>) -> Self {
        Self(definition)
    }
}

impl From<&ColorProfile> for ColorProfile {
    fn from(profile: &ColorProfile) -> Self {
        profile.clone()
    }
}

impl Deref for ColorProfile {
    type Target = dyn ProfileDefinition;

    fn deref(&self) -> &Self::Target {
        self.0.as_ref()
    }
}

impl PartialEq for ColorProfile {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for ColorProfile {}

impl Hash for ColorProfile {
    fn hash<H: Hasher>(&self, state: &mut H) {
        (Arc::as_ptr(&self.0) as *const ()).hash(state);
    }
}

impl fmt::Debug for ColorProfile {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("ColorProfile")
            .field(&(Arc::as_ptr(&self.0) as *const ()))
            .finish()
    }
}

/// Directed conversions supplied by a profile for one coordinate space.
///
/// The space identity is separate from its conversion data: two profiles can
/// interpret the same number of channels differently. The two PCS directions
/// are independent, and a direct link is optional rather than an inverse of
/// either direction. [`Self::space`] must identify the same space on every call;
/// clone a space stored by the definition rather than constructing a new one.
/// Definitions are `Send + Sync` so their handles can be shared by renderers.
pub trait ProfileDefinition: Send + Sync {
    fn space(&self) -> ColorSpace;
    fn pcs(&self) -> Pcs;
    fn space_to_pcs(
        &self,
        intent: IccRenderingIntent,
    ) -> Result<ColorTransform, ProfileConnectionError>;
    fn pcs_to_space(
        &self,
        intent: IccRenderingIntent,
    ) -> Result<ColorTransform, ProfileConnectionError>;

    /// A one-way conversion to a particular destination profile, if available.
    ///
    /// The destination profile and intent are part of a link's meaning. A link
    /// must not stand in for another merely because its space endpoints match.
    fn direct_to(
        &self,
        _destination: &ColorProfile,
        _intent: IccRenderingIntent,
    ) -> Result<Option<ColorTransform>, ProfileConnectionError> {
        Ok(None)
    }
}

/// Selects a direct link when the endpoints and intent match, otherwise joins
/// the source and destination profiles through their PCS representations.
pub fn connect_profiles(
    source: impl Into<ColorProfile>,
    destination: impl Into<ColorProfile>,
    source_intent: IccRenderingIntent,
    destination_intent: IccRenderingIntent,
) -> Result<ColorTransform, ProfileConnectionError> {
    let source = source.into();
    let destination = destination.into();
    if (source_intent == IccRenderingIntent::AbsoluteColorimetric)
        != (destination_intent == IccRenderingIntent::AbsoluteColorimetric)
    {
        return Err(ProfileConnectionError::IncompatibleIntentSpaces);
    }
    let source_space = source.space();
    let destination_space = destination.space();
    if source_intent == destination_intent
        && let Some(direct) = source.direct_to(&destination, source_intent)?
    {
        if direct.from() != &source_space || direct.to() != &destination_space {
            return Err(ProfileConnectionError::InvalidDirectLink);
        }
        return Ok(direct);
    }
    let mut transform = source.space_to_pcs(source_intent)?;
    if transform.from() != &source_space
        || transform.to() != &pcs_key_for_intent(source.pcs(), source_intent)
    {
        return Err(ProfileConnectionError::InvalidProfileTransform);
    }
    if source.pcs() != destination.pcs() {
        transform = transform.then(&ColorTransform::between_pcs_for_intent(
            source.pcs(),
            destination.pcs(),
            source_intent,
        )?)?;
    }
    let destination_transform = destination.pcs_to_space(destination_intent)?;
    if destination_transform.from() != &pcs_key_for_intent(destination.pcs(), destination_intent)
        || destination_transform.to() != &destination_space
    {
        return Err(ProfileConnectionError::InvalidProfileTransform);
    }
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
    InvalidDirectLink,
    InvalidProfileTransform,
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
            Self::InvalidProfileTransform => formatter
                .write_str("profile-to-PCS transform has incorrect coordinate-space endpoints"),
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
    use crate::icc_profile::IccProfile;
    use crate::space::{DisplayP3, LabD50, LinearSrgb, Srgb};
    use crate::transfer::{Transfer, TransferCurve, TransferStage};
    use std::collections::HashSet;

    #[test]
    fn predefined_profiles_share_handles_while_new_definitions_are_distinct() {
        let canonical = ColorProfile::from(Srgb);
        let another_reference = ColorProfile::from(Srgb);
        let custom = ColorProfile::new(Srgb);
        assert_eq!(canonical, another_reference);
        assert_eq!(canonical.space(), custom.space());
        assert_ne!(canonical, custom);
        let handles = HashSet::from([canonical.clone(), another_reference, custom]);
        assert_eq!(handles.len(), 2);
    }

    #[test]
    fn known_encoding_link_is_selected_without_a_pcs_round_trip() {
        let transform = connect_profiles(
            Srgb,
            LinearSrgb,
            IccRenderingIntent::RelativeColorimetric,
            IccRenderingIntent::RelativeColorimetric,
        )
        .unwrap();
        assert_eq!(
            transform.transfer().stages(),
            &[TransferStage::curves([TransferCurve::SRGB_DECODE; 3]).unwrap()]
        );
        assert_eq!(transform.from(), &ColorSpace::from(Srgb));
        assert_eq!(transform.to(), &ColorSpace::from(LinearSrgb));
    }

    #[test]
    fn builtin_direct_link_does_not_target_a_different_profile_of_the_same_space() {
        let bytes = moxcms::ColorProfile::new_srgb().encode().unwrap();
        let icc = IccProfile::from_bytes(&bytes)
            .unwrap()
            .with_device_space(Srgb.into())
            .unwrap();
        assert_eq!(icc.device_space(), Srgb.into());
        assert_ne!(icc.profile(), Srgb.into());
        assert!(
            LinearSrgb
                .direct_to(&icc.profile(), IccRenderingIntent::RelativeColorimetric)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn differing_primaries_fall_back_to_profile_pcs_directions() {
        let actual = connect_profiles(
            Srgb,
            DisplayP3,
            IccRenderingIntent::RelativeColorimetric,
            IccRenderingIntent::RelativeColorimetric,
        )
        .unwrap();
        let expected = Srgb
            .space_to_pcs(IccRenderingIntent::RelativeColorimetric)
            .unwrap()
            .then(
                &DisplayP3
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
        let transform = connect_profiles(
            &icc,
            LinearSrgb,
            IccRenderingIntent::RelativeColorimetric,
            IccRenderingIntent::RelativeColorimetric,
        )
        .unwrap();
        assert_eq!(transform.from(), &icc.device_space());
        assert_eq!(transform.to(), &ColorSpace::from(LinearSrgb));
        let converted = transform.transfer().evaluate(&[0.5; 3]).unwrap();
        assert!(converted.iter().all(|value| *value > 0.2 && *value < 0.23));
    }

    #[test]
    fn profiles_can_be_connected_after_type_erasure() {
        let source: ColorProfile = Srgb.into();
        let destination: ColorProfile = LabD50.into();
        let transform = connect_profiles(
            &source,
            &destination,
            IccRenderingIntent::RelativeColorimetric,
            IccRenderingIntent::RelativeColorimetric,
        )
        .unwrap();
        assert_eq!(transform.from(), &ColorSpace::from(Srgb));
        assert_eq!(transform.to(), &ColorSpace::from(LabD50));
        assert_eq!(transform.transfer().evaluate(&[1.0; 3]).unwrap()[0], 100.0);
    }

    #[test]
    fn direct_link_is_used_only_for_its_declared_intent() {
        struct LinkedSpace;

        impl ProfileDefinition for LinkedSpace {
            fn space(&self) -> ColorSpace {
                Srgb.into()
            }

            fn pcs(&self) -> Pcs {
                Pcs::XyzD50
            }

            fn space_to_pcs(
                &self,
                _: IccRenderingIntent,
            ) -> Result<ColorTransform, ProfileConnectionError> {
                Srgb.space_to_pcs(IccRenderingIntent::RelativeColorimetric)
            }

            fn pcs_to_space(
                &self,
                _: IccRenderingIntent,
            ) -> Result<ColorTransform, ProfileConnectionError> {
                unreachable!()
            }

            fn direct_to(
                &self,
                destination: &ColorProfile,
                intent: IccRenderingIntent,
            ) -> Result<Option<ColorTransform>, ProfileConnectionError> {
                if *destination == ColorProfile::from(LinearSrgb)
                    && intent == IccRenderingIntent::Saturation
                {
                    return Ok(Some(ColorTransform::new(
                        self.space(),
                        destination.space(),
                        Transfer::new(3, Vec::<TransferStage>::new())?,
                    )?));
                }
                Ok(None)
            }
        }

        let direct = connect_profiles(
            ColorProfile::new(LinkedSpace),
            LinearSrgb,
            IccRenderingIntent::Saturation,
            IccRenderingIntent::Saturation,
        )
        .unwrap();
        assert!(direct.transfer().stages().is_empty());

        let fallback = connect_profiles(
            ColorProfile::new(LinkedSpace),
            LinearSrgb,
            IccRenderingIntent::Saturation,
            IccRenderingIntent::RelativeColorimetric,
        )
        .unwrap();
        assert!(!fallback.transfer().stages().is_empty());
    }

    #[test]
    fn unsupported_builtin_intent_is_not_misrepresented_as_relative() {
        assert!(matches!(
            connect_profiles(
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
    fn malformed_direct_link_cannot_escape_with_a_matching_channel_count() {
        struct InvalidLink;

        impl ProfileDefinition for InvalidLink {
            fn space(&self) -> ColorSpace {
                Srgb.into()
            }

            fn pcs(&self) -> Pcs {
                Pcs::XyzD50
            }

            fn space_to_pcs(
                &self,
                _: IccRenderingIntent,
            ) -> Result<ColorTransform, ProfileConnectionError> {
                unreachable!()
            }

            fn pcs_to_space(
                &self,
                _: IccRenderingIntent,
            ) -> Result<ColorTransform, ProfileConnectionError> {
                unreachable!()
            }

            fn direct_to(
                &self,
                _: &ColorProfile,
                _: IccRenderingIntent,
            ) -> Result<Option<ColorTransform>, ProfileConnectionError> {
                Ok(Some(ColorTransform::new(
                    DisplayP3.into(),
                    LinearSrgb.into(),
                    Transfer::new(3, Vec::<TransferStage>::new()).unwrap(),
                )?))
            }
        }

        assert!(matches!(
            connect_profiles(
                ColorProfile::new(InvalidLink),
                LinearSrgb,
                IccRenderingIntent::RelativeColorimetric,
                IccRenderingIntent::RelativeColorimetric,
            ),
            Err(ProfileConnectionError::InvalidDirectLink)
        ));
    }

    #[test]
    fn malformed_pcs_link_cannot_escape_with_a_matching_channel_count() {
        struct InvalidPcs;

        impl ProfileDefinition for InvalidPcs {
            fn space(&self) -> ColorSpace {
                Srgb.into()
            }

            fn pcs(&self) -> Pcs {
                Pcs::XyzD50
            }

            fn space_to_pcs(
                &self,
                _: IccRenderingIntent,
            ) -> Result<ColorTransform, ProfileConnectionError> {
                Ok(ColorTransform::new(
                    Srgb.into(),
                    LabD50.into(),
                    Transfer::new(3, Vec::<TransferStage>::new())?,
                )?)
            }

            fn pcs_to_space(
                &self,
                _: IccRenderingIntent,
            ) -> Result<ColorTransform, ProfileConnectionError> {
                unreachable!()
            }
        }

        assert!(matches!(
            connect_profiles(
                ColorProfile::new(InvalidPcs),
                LinearSrgb,
                IccRenderingIntent::RelativeColorimetric,
                IccRenderingIntent::RelativeColorimetric,
            ),
            Err(ProfileConnectionError::InvalidProfileTransform)
        ));
    }
}
