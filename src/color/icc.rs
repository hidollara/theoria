use std::{error::Error, fmt};

use super::{Transfer, TransferError};

/// The D50 profile connection space declared by an ICC.1 profile.
///
/// These are different coordinate systems even though each has three
/// channels. The coordinates at an [`IccTransfer`] boundary are actual XYZ
/// or Lab values, not the normalized integer encodings inside ICC LUT tags.
/// Converting between the two spaces requires an explicit transfer.
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
    /// Profiles with different PCS representations must be bridged explicitly;
    /// identical channel counts do not imply compatible coordinates.
    pub fn connect(&self, destination: &Self) -> Result<Transfer, IccTransferError> {
        if self.direction != IccTransferDirection::DeviceToPcs
            || destination.direction != IccTransferDirection::PcsToDevice
        {
            return Err(IccTransferError::InvalidDirections);
        }
        if self.pcs != destination.pcs {
            return Err(IccTransferError::IncompatiblePcs);
        }
        self.transfer
            .then(&destination.transfer)
            .map_err(IccTransferError::Transfer)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IccTransferError {
    InvalidPcsChannels { actual: usize },
    InvalidDirections,
    IncompatiblePcs,
    Transfer(TransferError),
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
            Self::IncompatiblePcs => formatter.write_str("ICC connection spaces differ"),
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
    fn wrong_direction_and_incompatible_pcs_are_rejected() {
        let identity = Transfer::new(3, Vec::<TransferStage>::new()).unwrap();
        let xyz = IccTransfer::device_to_pcs(IccPcs::XyzD50, identity.clone()).unwrap();
        let lab = IccTransfer::pcs_to_device(IccPcs::LabD50, identity.clone()).unwrap();
        assert_eq!(xyz.connect(&lab), Err(IccTransferError::IncompatiblePcs));
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
