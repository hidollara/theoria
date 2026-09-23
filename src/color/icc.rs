use std::{error::Error, fmt};

use super::{Transfer, TransferError, TransferStage};

/// The D50 profile connection space declared by an ICC.1 profile.
///
/// These are different coordinate systems even though each has three
/// channels. The coordinates at an [`IccTransfer`] boundary are actual XYZ
/// or Lab values, not the normalized integer encodings inside ICC LUT tags.
/// Converting between the two spaces requires a PCS transfer; [`IccTransfer::connect`]
/// inserts one when the spaces differ.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IccPcs {
    XyzD50,
    LabD50,
}

/// The direction of a profile's transfer relative to its device coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IccTransferDirection {
    DeviceToPcs,
    PcsToDevice,
}

/// A validated, directional ICC profile transfer.
///
/// This may be built directly or obtained from [`super::IccProfile`] after
/// parsing profile bytes and selecting a rendering intent. It associates the
/// selected [`Transfer`] with its PCS meaning, which channel counts alone
/// cannot express. Direct construction requires the caller to supply actual
/// D50 XYZ/Lab values at the PCS boundary.
#[derive(Clone, Debug, PartialEq)]
pub struct IccTransfer {
    pcs: IccPcs,
    direction: IccTransferDirection,
    transfer: Transfer,
}

impl IccTransfer {
    pub fn device_to_pcs(pcs: IccPcs, transfer: Transfer) -> Result<Self, IccTransferError> {
        if transfer.output_channels() != 3 {
            return Err(IccTransferError::InvalidPcsChannels {
                actual: transfer.output_channels(),
            });
        }
        Ok(Self {
            pcs,
            direction: IccTransferDirection::DeviceToPcs,
            transfer,
        })
    }

    pub fn pcs_to_device(pcs: IccPcs, transfer: Transfer) -> Result<Self, IccTransferError> {
        if transfer.input_channels() != 3 {
            return Err(IccTransferError::InvalidPcsChannels {
                actual: transfer.input_channels(),
            });
        }
        Ok(Self {
            pcs,
            direction: IccTransferDirection::PcsToDevice,
            transfer,
        })
    }

    pub const fn pcs(&self) -> IccPcs {
        self.pcs
    }

    pub const fn direction(&self) -> IccTransferDirection {
        self.direction
    }

    pub const fn transfer(&self) -> &Transfer {
        &self.transfer
    }

    pub fn into_transfer(self) -> Transfer {
        self.transfer
    }

    /// Connects a device-to-PCS transfer to a PCS-to-device transfer.
    ///
    /// Profiles with different PCS representations are bridged through actual
    /// D50 XYZ/Lab coordinates; identical channel counts alone do not imply
    /// compatible coordinates.
    pub fn connect(&self, destination: &Self) -> Result<Transfer, IccTransferError> {
        if self.direction != IccTransferDirection::DeviceToPcs
            || destination.direction != IccTransferDirection::PcsToDevice
        {
            return Err(IccTransferError::InvalidDirections);
        }
        let bridge = match (self.pcs, destination.pcs) {
            (IccPcs::XyzD50, IccPcs::LabD50) => Some(TransferStage::XyzD50ToLabD50),
            (IccPcs::LabD50, IccPcs::XyzD50) => Some(TransferStage::LabD50ToXyzD50),
            _ => None,
        };
        let first = if let Some(stage) = bridge {
            self.transfer.then(&Transfer::new(3, [stage])?)?
        } else {
            self.transfer.clone()
        };
        first.then(&destination.transfer).map_err(Into::into)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IccTransferError {
    InvalidPcsChannels { actual: usize },
    InvalidDirections,
    Transfer(TransferError),
}

impl From<TransferError> for IccTransferError {
    fn from(error: TransferError) -> Self {
        Self::Transfer(error)
    }
}

impl fmt::Display for IccTransferError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPcsChannels { actual } => {
                write!(formatter, "ICC PCS requires three channels, got {actual}")
            }
            Self::InvalidDirections => {
                formatter.write_str("ICC transfer directions cannot be connected")
            }
            Self::Transfer(error) => error.fmt(formatter),
        }
    }
}

impl Error for IccTransferError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Transfer(error) => Some(error),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::{TransferClut, TransferStage};

    #[test]
    fn four_channel_device_can_connect_through_matching_pcs() {
        let source = Transfer::new(
            4,
            [TransferStage::Clut(
                TransferClut::new([2, 2, 2, 2], 3, vec![0.5; 48]).unwrap(),
            )],
        )
        .unwrap();
        let destination = Transfer::new(3, Vec::<TransferStage>::new()).unwrap();
        let source = IccTransfer::device_to_pcs(IccPcs::LabD50, source).unwrap();
        let destination = IccTransfer::pcs_to_device(IccPcs::LabD50, destination).unwrap();
        let connected = source.connect(&destination).unwrap();
        assert_eq!(connected.input_channels(), 4);
        assert_eq!(connected.output_channels(), 3);
        assert_eq!(connected.evaluate(&[0.0; 4]).unwrap(), vec![0.5; 3]);
    }

    #[test]
    fn wrong_direction_is_rejected_and_different_pcs_is_converted() {
        let identity = Transfer::new(3, Vec::<TransferStage>::new()).unwrap();
        let xyz = IccTransfer::device_to_pcs(IccPcs::XyzD50, identity.clone()).unwrap();
        let lab = IccTransfer::pcs_to_device(IccPcs::LabD50, identity.clone()).unwrap();
        let connected = xyz.connect(&lab).unwrap();
        let white = connected.evaluate(&[0.9642, 1.0, 0.8249]).unwrap();
        assert!((white[0] - 100.0).abs() < 1e-12);
        assert!(white[1].abs() < 1e-12);
        assert!(white[2].abs() < 1e-12);
        let lab_source = IccTransfer::device_to_pcs(IccPcs::LabD50, identity.clone()).unwrap();
        let xyz_destination = IccTransfer::pcs_to_device(IccPcs::XyzD50, identity.clone()).unwrap();
        let xyz_white = lab_source
            .connect(&xyz_destination)
            .unwrap()
            .evaluate(&[100.0, 0.0, 0.0])
            .unwrap();
        for (actual, expected) in xyz_white.into_iter().zip([0.9642, 1.0, 0.8249]) {
            assert!((actual - expected).abs() < 1e-12);
        }
        assert_eq!(xyz.connect(&xyz), Err(IccTransferError::InvalidDirections));
        assert_eq!(lab.connect(&xyz), Err(IccTransferError::InvalidDirections));
        assert!(
            IccTransfer::device_to_pcs(
                IccPcs::XyzD50,
                Transfer::new(2, Vec::<TransferStage>::new()).unwrap()
            )
            .is_err()
        );
    }
}
