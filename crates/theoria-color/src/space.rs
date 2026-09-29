use std::{
    error::Error,
    fmt,
    hash::{Hash, Hasher},
    sync::Arc,
};

use super::icc_profile::IccProfileError;
use super::model::{ColorModel, TypedColorModel};
use super::transfer::{Transfer, TransferError};

mod builtin;

pub use builtin::{DisplayP3, LabD50, LinearDisplayP3, LinearSrgb, Srgb, XyzD50, XyzD65};

/// Associates a statically typed color space with its coordinate model.
///
/// This constrains authored coordinates at compile time and supplies their
/// runtime color space for rendering. The runtime space's model must match
/// `Model`, and conversion must return the same space on repeated calls.
pub trait TypedColorSpace: Into<ColorSpace> + 'static {
    type Model: TypedColorModel;
}

/// The coordinate representation used to connect color spaces.
///
/// Built-in D65 spaces connect through XYZ D65 without adapting to the ICC
/// D50 PCS. Loaded ICC profiles use only XYZ D50 or Lab D50 here; their
/// normalized LUT encodings are decoded before reaching this boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ConnectionSpace {
    XyzD65,
    XyzD50,
    LabD50,
}

impl ConnectionSpace {
    fn transfer_to(self, destination: Self) -> Transfer {
        if self == destination {
            return Transfer::IDENTITY;
        }
        let to_d50 = match self {
            Self::XyzD65 => XyzD65::transfer_to_xyz_d50(),
            Self::XyzD50 => Transfer::IDENTITY,
            Self::LabD50 => LabD50::transfer_to_xyz_d50(),
        };
        let from_d50 = match destination {
            Self::XyzD65 => XyzD50::transfer_to_xyz_d65(),
            Self::XyzD50 => Transfer::IDENTITY,
            Self::LabD50 => XyzD50::transfer_to_lab_d50(),
        };
        to_d50
            .then(&from_d50)
            .expect("built-in connection spaces have matching channels")
    }
}

/// A shared runtime color space with a profile-defined connection space.
///
/// Its selected profile supplies the coordinate model and directed connection
/// transforms. Built-in D65 spaces can connect without a D50 adaptation.
/// Two spaces selected from the same ICC data with different intents are
/// distinct, even though their input coordinates are the same. The colors
/// reached in the ICC PCS are determined by those conversions, not merely by
/// the coordinate model.
///
/// This handle does not itself validate the range of an individual coordinate
/// value or guarantee that the profile supports both directions for the
/// selected intent. Equality is the identity of the shared profile allocation,
/// which clones retain.
#[derive(Clone)]
pub struct ColorSpace {
    profile: Arc<dyn ColorProfile>,
}

impl ColorSpace {
    /// Owns a selected profile as a new color space.
    /// Named spaces use their canonical `Into<ColorSpace>` conversion instead.
    pub fn new(profile: impl ColorProfile + 'static) -> Self {
        assert!(profile.model().channels() > 0, "color model needs channels");
        Self {
            profile: Arc::new(profile),
        }
    }

    pub fn channels(&self) -> usize {
        self.model().channels()
    }

    pub fn model(&self) -> ColorModel {
        self.profile.model()
    }

    pub fn connection_space(&self) -> ConnectionSpace {
        self.profile.connection_space()
    }

    /// Returns a directed transfer to this profile's connection space.
    pub fn transfer_to_connection_space(&self) -> Result<Transfer, SpaceConnectionError> {
        let transfer = self.profile.transfer_to_connection_space()?;
        require_transfer_channels(&transfer, self.channels(), 3)?;
        Ok(transfer)
    }

    /// Returns a raw directed transfer. It is not inferred by inverting the
    /// transfer to the connection space.
    pub fn transfer_from_connection_space(&self) -> Result<Transfer, SpaceConnectionError> {
        let transfer = self.profile.transfer_from_connection_space()?;
        require_transfer_channels(&transfer, 3, self.channels())?;
        Ok(transfer)
    }

    pub fn direct_to(
        &self,
        destination: &ColorSpace,
    ) -> Result<Option<Transfer>, SpaceConnectionError> {
        if self == destination {
            return Ok(Some(Transfer::IDENTITY));
        }
        self.profile
            .direct_transfer_to(destination)?
            .map(|transfer| {
                require_transfer_channels(&transfer, self.channels(), destination.channels())?;
                Ok(transfer)
            })
            .transpose()
    }

    /// Selects a direct link or connects through the profiles' connection spaces.
    pub fn transfer_to(
        &self,
        destination: impl Into<ColorSpace>,
    ) -> Result<Transfer, SpaceConnectionError> {
        let destination = destination.into();
        if let Some(direct) = self.direct_to(&destination)? {
            return Ok(direct);
        }
        let bridge = self
            .connection_space()
            .transfer_to(destination.connection_space());
        let destination_transform = destination.transfer_from_connection_space()?;
        Ok(self
            .transfer_to_connection_space()?
            .then(&bridge)?
            .then(&destination_transform)?)
    }
}

fn require_channels(expected: usize, actual: usize) -> Result<(), SpaceConnectionError> {
    if expected == actual {
        Ok(())
    } else {
        Err(SpaceConnectionError::Channels { expected, actual })
    }
}

fn require_transfer_channels(
    transfer: &Transfer,
    input: usize,
    output: usize,
) -> Result<(), SpaceConnectionError> {
    if transfer.is_empty() {
        require_channels(input, output)
    } else {
        require_channels(input, transfer.input_channels().expect("nonempty transfer"))?;
        require_channels(
            output,
            transfer.output_channels().expect("nonempty transfer"),
        )
    }
}

impl From<&ColorSpace> for ColorSpace {
    fn from(space: &ColorSpace) -> Self {
        space.clone()
    }
}

impl PartialEq for ColorSpace {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.profile, &other.profile)
    }
}

impl Eq for ColorSpace {}

impl Hash for ColorSpace {
    fn hash<H: Hasher>(&self, state: &mut H) {
        (Arc::as_ptr(&self.profile) as *const ()).hash(state);
    }
}

impl fmt::Debug for ColorSpace {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("ColorSpace")
            .field(&(Arc::as_ptr(&self.profile) as *const ()))
            .finish()
    }
}

/// A coordinate model and the conversions for one selected interpretation.
///
/// An [`IccProfile`](super::icc_profile::IccProfile) can define multiple
/// implementations of this trait by selecting different rendering intents.
/// The two connection-space directions are independent, and a direct link is optional.
/// Definitions are `Send + Sync` so their handles can be shared by renderers.
/// `model` and `connection_space` must remain stable for the lifetime of
/// the profile so its [`ColorSpace`] handle has stable identity.
pub trait ColorProfile: Send + Sync {
    fn model(&self) -> ColorModel;
    fn connection_space(&self) -> ConnectionSpace;
    fn transfer_to_connection_space(&self) -> Result<Transfer, SpaceConnectionError>;
    fn transfer_from_connection_space(&self) -> Result<Transfer, SpaceConnectionError>;

    /// A one-way conversion to a particular destination space, if available.
    ///
    /// `ColorSpace::direct_to` handles the identity link before calling this hook.
    /// The destination's selected profile is part of a link's meaning. A link
    /// must not stand in for another merely because its coordinate models match.
    fn direct_transfer_to(
        &self,
        _destination: &ColorSpace,
    ) -> Result<Option<Transfer>, SpaceConnectionError> {
        Ok(None)
    }
}

/// A failure to construct a conversion between color spaces.
#[derive(Debug)]
pub enum SpaceConnectionError {
    Channels { expected: usize, actual: usize },
    Transfer(TransferError),
    Icc(IccProfileError),
}

impl From<TransferError> for SpaceConnectionError {
    fn from(error: TransferError) -> Self {
        Self::Transfer(error)
    }
}

impl From<IccProfileError> for SpaceConnectionError {
    fn from(error: IccProfileError) -> Self {
        Self::Icc(error)
    }
}

impl fmt::Display for SpaceConnectionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
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

impl Error for SpaceConnectionError {
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
    use crate::icc_profile::{IccProfile, IccRenderingIntent};
    use crate::transfer::{Transfer, TransferCurve, TransferStage};
    use std::collections::HashSet;

    #[test]
    fn builtin_spaces_share_handles_while_other_definitions_are_distinct() {
        let canonical = ColorSpace::from(Srgb);
        let another_reference = ColorSpace::from(Srgb);
        let custom = IccProfile::from_bytes(&moxcms::ColorProfile::new_srgb().encode().unwrap())
            .unwrap()
            .color_space(IccRenderingIntent::RelativeColorimetric);
        assert_eq!(canonical, another_reference);
        assert_eq!(canonical.model(), ColorModel::Rgb);
        assert_ne!(canonical, custom);
        let handles = HashSet::from([canonical.clone(), another_reference, custom]);
        assert_eq!(handles.len(), 2);
    }

    #[test]
    fn known_encoding_link_is_selected_without_a_pcs_round_trip() {
        let transform = ColorSpace::from(Srgb).transfer_to(LinearSrgb).unwrap();
        assert_eq!(
            transform.stages(),
            &[TransferStage::curves(vec![TransferCurve::SRGB_DECODE; 3]).unwrap()]
        );
        let expected = 0.214_041_140_482_232_55;
        for actual in transform.evaluate(&[0.5; 3]).unwrap() {
            assert!((actual - expected).abs() < 1e-12);
        }
    }

    #[test]
    fn each_rgb_profile_declares_its_encoding_counterpart() {
        for (source, destination, curve) in [
            (
                ColorSpace::from(Srgb),
                ColorSpace::from(LinearSrgb),
                TransferCurve::SRGB_DECODE,
            ),
            (
                ColorSpace::from(LinearSrgb),
                ColorSpace::from(Srgb),
                TransferCurve::SRGB_ENCODE,
            ),
            (
                ColorSpace::from(DisplayP3),
                ColorSpace::from(LinearDisplayP3),
                TransferCurve::SRGB_DECODE,
            ),
            (
                ColorSpace::from(LinearDisplayP3),
                ColorSpace::from(DisplayP3),
                TransferCurve::SRGB_ENCODE,
            ),
        ] {
            let expected = Transfer::new([TransferStage::curves(vec![curve; 3]).unwrap()]).unwrap();
            assert_eq!(source.direct_to(&destination).unwrap(), Some(expected));
        }
    }

    #[test]
    fn matching_connection_space_uses_an_empty_transfer() {
        for connection in [
            ConnectionSpace::XyzD65,
            ConnectionSpace::XyzD50,
            ConnectionSpace::LabD50,
        ] {
            assert!(connection.transfer_to(connection).is_empty());
        }
        assert!(
            ColorSpace::from(XyzD50)
                .transfer_to(XyzD50)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn d65_and_lab_connection_spaces_bridge_through_xyz_d50() {
        let expected = XyzD65::transfer_to_xyz_d50()
            .then(&XyzD50::transfer_to_lab_d50())
            .unwrap();
        assert_eq!(
            ConnectionSpace::XyzD65.transfer_to(ConnectionSpace::LabD50),
            expected
        );
        assert_eq!(
            ColorSpace::from(XyzD65).transfer_to(LabD50).unwrap(),
            expected
        );
    }

    #[test]
    fn every_space_has_a_direct_identity_link_to_itself() {
        let profile =
            IccProfile::from_bytes(&moxcms::ColorProfile::new_srgb().encode().unwrap()).unwrap();
        let spaces = [
            ColorSpace::from(Srgb),
            profile.color_space(IccRenderingIntent::RelativeColorimetric),
        ];
        for space in spaces {
            assert_eq!(space.direct_to(&space).unwrap(), Some(Transfer::IDENTITY));
            assert_eq!(space.transfer_to(&space).unwrap(), Transfer::IDENTITY);
        }
    }

    #[test]
    fn xyz_and_lab_spaces_provide_direct_transfers() {
        let xyz = ColorSpace::from(XyzD50);
        let lab = ColorSpace::from(LabD50);
        assert_eq!(
            xyz.direct_to(&lab).unwrap(),
            Some(XyzD50::transfer_to_lab_d50())
        );
        assert_eq!(
            lab.direct_to(&xyz).unwrap(),
            Some(LabD50::transfer_to_xyz_d50())
        );
        assert_eq!(
            xyz.transfer_to(&lab).unwrap(),
            XyzD50::transfer_to_lab_d50()
        );
        assert_eq!(
            lab.transfer_to(&xyz).unwrap(),
            LabD50::transfer_to_xyz_d50()
        );
    }

    #[test]
    fn builtin_direct_link_does_not_target_a_loaded_icc_space() {
        let bytes = moxcms::ColorProfile::new_srgb().encode().unwrap();
        let icc = IccProfile::from_bytes(&bytes).unwrap();
        assert_ne!(
            icc.color_space(IccRenderingIntent::RelativeColorimetric),
            Srgb.into()
        );
        assert!(
            ColorSpace::from(LinearSrgb)
                .direct_to(&icc.color_space(IccRenderingIntent::RelativeColorimetric))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn differing_primaries_share_xyz_d65_connection_space() {
        let actual = ColorSpace::from(Srgb).transfer_to(DisplayP3).unwrap();
        let expected = ColorSpace::from(Srgb)
            .transfer_to_connection_space()
            .unwrap()
            .then(
                &ColorSpace::from(DisplayP3)
                    .transfer_from_connection_space()
                    .unwrap(),
            )
            .unwrap();
        assert_eq!(actual, expected);
        // Decode, source RGB to D65 XYZ, D65 XYZ to destination RGB, encode.
        // No D65-to-D50 or D50-to-D65 adaptation is inserted.
        assert_eq!(actual.stages().len(), 4);
    }

    #[test]
    fn icc_and_builtin_spaces_bridge_their_connection_spaces() {
        let bytes = moxcms::ColorProfile::new_srgb().encode().unwrap();
        let icc = IccProfile::from_bytes(&bytes).unwrap();
        let transform = icc
            .color_space(IccRenderingIntent::RelativeColorimetric)
            .transfer_to(LinearSrgb)
            .unwrap();
        let converted = transform.evaluate(&[0.5; 3]).unwrap();
        assert!(converted.iter().all(|value| *value > 0.2 && *value < 0.23));
    }

    #[test]
    fn predefined_and_icc_spaces_round_trip_through_connection_spaces() {
        let bytes = moxcms::ColorProfile::new_srgb().encode().unwrap();
        let icc = IccProfile::from_bytes(&bytes).unwrap();
        let icc = icc.color_space(IccRenderingIntent::RelativeColorimetric);
        let forward = ColorSpace::from(Srgb).transfer_to(&icc).unwrap();
        let backward = icc.transfer_to(Srgb).unwrap();
        let sample = [0.3, 0.5, 0.7];
        let converted = forward.evaluate(&sample).unwrap();
        let restored = backward.evaluate(&converted).unwrap();
        for (actual, expected) in restored.into_iter().zip(sample) {
            assert!((actual - expected).abs() < 0.004);
        }
    }

    #[test]
    fn spaces_can_be_connected_after_type_erasure() {
        let source: ColorSpace = Srgb.into();
        let destination: ColorSpace = LabD50.into();
        let transform = source.transfer_to(&destination).unwrap();
        assert_eq!(transform.evaluate(&[1.0; 3]).unwrap()[0], 100.0);
    }

    #[test]
    fn connection_bridge_round_trips_between_rgb_and_lab() {
        let to_lab = ColorSpace::from(LinearSrgb).transfer_to(LabD50).unwrap();
        let to_rgb = ColorSpace::from(LabD50).transfer_to(LinearSrgb).unwrap();
        let sample = [0.2, 0.4, 0.6];
        let lab = to_lab.evaluate(&sample).unwrap();
        let restored = to_rgb.evaluate(&lab).unwrap();
        for (actual, expected) in restored.into_iter().zip(sample) {
            assert!((actual - expected).abs() < 1e-7);
        }
    }

    #[test]
    fn builtin_spaces_match_css_reference_coordinates() {
        // CSS Color 4 §10.9 gives these equivalent coordinates for #7654CD.
        // https://www.w3.org/TR/css-color-4/#predefined
        let srgb = [118.0 / 255.0, 84.0 / 255.0, 205.0 / 255.0];
        for (destination, expected, tolerance) in [
            (ColorSpace::from(XyzD50), [0.2005, 0.14089, 0.4472], 0.0005),
            (
                ColorSpace::from(XyzD65),
                [0.21661, 0.14602, 0.59452],
                0.0005,
            ),
            (ColorSpace::from(LabD50), [44.36, 36.05, -58.99], 0.05),
        ] {
            let transfer = ColorSpace::from(Srgb).transfer_to(destination).unwrap();
            let actual = transfer.evaluate(&srgb).unwrap();
            for (actual, expected) in actual.into_iter().zip(expected) {
                assert!(
                    (actual - expected).abs() < tolerance,
                    "{actual} != {expected}"
                );
            }
        }
    }

    #[test]
    fn direct_link_is_used_only_for_its_declared_destination() {
        struct LinkedSpace(Option<ColorSpace>);

        impl ColorProfile for LinkedSpace {
            fn model(&self) -> ColorModel {
                ColorModel::Rgb
            }

            fn connection_space(&self) -> ConnectionSpace {
                ConnectionSpace::XyzD65
            }

            fn transfer_to_connection_space(&self) -> Result<Transfer, SpaceConnectionError> {
                ColorSpace::from(Srgb).transfer_to_connection_space()
            }

            fn transfer_from_connection_space(&self) -> Result<Transfer, SpaceConnectionError> {
                unreachable!()
            }

            fn direct_transfer_to(
                &self,
                destination: &ColorSpace,
            ) -> Result<Option<Transfer>, SpaceConnectionError> {
                if self.0.as_ref() == Some(destination) {
                    return Ok(Some(Transfer::IDENTITY));
                }
                Ok(None)
            }
        }

        let destination = ColorSpace::new(LinkedSpace(None));
        let source = ColorSpace::new(LinkedSpace(Some(destination.clone())));
        let direct = source.transfer_to(&destination).unwrap();
        assert!(direct.stages().is_empty());

        let fallback = source.transfer_to(LinearSrgb).unwrap();
        assert!(!fallback.stages().is_empty());
    }

    #[test]
    fn direct_transfer_with_wrong_channel_count_is_rejected() {
        struct InvalidLink;

        impl ColorProfile for InvalidLink {
            fn model(&self) -> ColorModel {
                ColorModel::Rgb
            }

            fn connection_space(&self) -> ConnectionSpace {
                ConnectionSpace::XyzD50
            }

            fn transfer_to_connection_space(&self) -> Result<Transfer, SpaceConnectionError> {
                unreachable!()
            }

            fn transfer_from_connection_space(&self) -> Result<Transfer, SpaceConnectionError> {
                unreachable!()
            }

            fn direct_transfer_to(
                &self,
                _: &ColorSpace,
            ) -> Result<Option<Transfer>, SpaceConnectionError> {
                Ok(Some(Transfer::new([TransferStage::Clamp(4)])?))
            }
        }

        assert!(matches!(
            ColorSpace::new(InvalidLink).transfer_to(LinearSrgb),
            Err(SpaceConnectionError::Channels { .. })
        ));
    }

    #[test]
    fn profile_transfer_with_wrong_channel_count_is_rejected() {
        struct InvalidPcs;

        impl ColorProfile for InvalidPcs {
            fn model(&self) -> ColorModel {
                ColorModel::Cmyk
            }

            fn connection_space(&self) -> ConnectionSpace {
                ConnectionSpace::XyzD50
            }

            fn transfer_to_connection_space(&self) -> Result<Transfer, SpaceConnectionError> {
                Ok(Transfer::IDENTITY)
            }

            fn transfer_from_connection_space(&self) -> Result<Transfer, SpaceConnectionError> {
                unreachable!()
            }
        }

        assert!(matches!(
            ColorSpace::new(InvalidPcs).transfer_to(LinearSrgb),
            Err(SpaceConnectionError::Channels { .. })
        ));
    }
}
