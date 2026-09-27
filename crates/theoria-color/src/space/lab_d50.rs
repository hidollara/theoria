use super::{
    ColorProfile, ColorSpace, ColorSpaceKey, ColorTransform, IccPcs, IccRenderingIntent,
    ProfileConnectionError, require_relative,
};
use crate::model::Lab;

named_space!(LabD50, Lab);
predefined_space!(LabD50, LAB_D50_KEY);

impl ColorProfile for LabD50 {
    fn space(&self) -> ColorSpaceKey {
        (*self).into()
    }

    fn pcs(&self) -> IccPcs {
        IccPcs::LabD50
    }

    fn space_to_pcs(
        &self,
        intent: IccRenderingIntent,
    ) -> Result<ColorTransform, ProfileConnectionError> {
        require_relative(intent)?;
        Ok(ColorTransform::between_pcs(self.pcs(), self.pcs())?)
    }

    fn pcs_to_space(
        &self,
        intent: IccRenderingIntent,
    ) -> Result<ColorTransform, ProfileConnectionError> {
        self.space_to_pcs(intent)
    }
}
