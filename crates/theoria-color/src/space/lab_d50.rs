use super::{ColorProfile, Pcs, SpaceConnectionError, Transfer, TypedColorSpace};
use crate::model::Lab;

named_space!(LabD50, Lab);
predefined_space!(LabD50, LabD50Profile);

impl ColorProfile for LabD50Profile {
    fn model(&self) -> crate::model::ColorModel {
        crate::model::ColorModel::Lab
    }

    fn pcs(&self) -> Pcs {
        Pcs::LabD50
    }

    fn space_to_pcs_transfer(&self) -> Result<Transfer, SpaceConnectionError> {
        Ok(Transfer::new(3, [])?)
    }

    fn pcs_to_space_transfer(&self) -> Result<Transfer, SpaceConnectionError> {
        self.space_to_pcs_transfer()
    }
}
