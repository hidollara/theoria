use super::{
    IccRenderingIntent, Pcs, ProfileConnectionError, ProfileDefinition, Transfer, TypedColorSpace,
    require_relative,
};
use crate::model::Lab;

named_space!(LabD50, Lab);
predefined_space!(LabD50, LabD50Profile);

impl ProfileDefinition for LabD50Profile {
    fn model(&self) -> crate::model::ColorModel {
        crate::model::ColorModel::Lab
    }

    fn pcs(&self) -> Pcs {
        Pcs::LabD50
    }

    fn space_to_pcs_transfer(
        &self,
        intent: IccRenderingIntent,
    ) -> Result<Transfer, ProfileConnectionError> {
        require_relative(intent)?;
        Ok(Transfer::new(3, [])?)
    }

    fn pcs_to_space_transfer(
        &self,
        intent: IccRenderingIntent,
    ) -> Result<Transfer, ProfileConnectionError> {
        self.space_to_pcs_transfer(intent)
    }
}
