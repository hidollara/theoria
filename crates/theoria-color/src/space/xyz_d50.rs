use super::{
    ColorProfile, IccRenderingIntent, Pcs, SpaceConnectionError, Transfer, TypedColorSpace,
    require_relative,
};
use crate::model::Xyz;

named_space!(XyzD50, Xyz);
predefined_space!(XyzD50, XyzD50Profile);

impl ColorProfile for XyzD50Profile {
    fn model(&self) -> crate::model::ColorModel {
        crate::model::ColorModel::Xyz
    }

    fn pcs(&self) -> Pcs {
        Pcs::XyzD50
    }

    fn space_to_pcs_transfer(
        &self,
        intent: IccRenderingIntent,
    ) -> Result<Transfer, SpaceConnectionError> {
        require_relative(intent)?;
        Ok(Transfer::new(3, [])?)
    }

    fn pcs_to_space_transfer(
        &self,
        intent: IccRenderingIntent,
    ) -> Result<Transfer, SpaceConnectionError> {
        self.space_to_pcs_transfer(intent)
    }
}
