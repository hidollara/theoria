use super::{
    IccRenderingIntent, Pcs, ProfileConnectionError, ProfileDefinition, Transfer, TypedColorSpace,
    require_relative,
};
use crate::model::Lab;

named_space!(LabD50, Lab);
predefined_space!(LabD50, LabD50Profile);

impl ProfileDefinition for LabD50Profile {
    fn channels(&self) -> usize {
        3
    }

    fn pcs(&self) -> Pcs {
        Pcs::LabD50
    }

    fn to_pcs_transfer(
        &self,
        intent: IccRenderingIntent,
    ) -> Result<Transfer, ProfileConnectionError> {
        require_relative(intent)?;
        Ok(Transfer::new(3, [])?)
    }

    fn from_pcs_transfer(
        &self,
        intent: IccRenderingIntent,
    ) -> Result<Transfer, ProfileConnectionError> {
        self.to_pcs_transfer(intent)
    }
}
