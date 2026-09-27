use super::{
    ColorProfile, ColorSpace, ColorTransform, IccRenderingIntent, Pcs, ProfileConnectionError,
    ProfileDefinition, TypedColorSpace, require_relative,
};
use crate::model::Lab;

named_space!(LabD50, Lab);
predefined_space!(LabD50, LabD50Profile, LAB_D50_KEY);

impl ProfileDefinition for LabD50Profile {
    fn space(&self) -> ColorSpace {
        LabD50.into()
    }

    fn pcs(&self) -> Pcs {
        Pcs::LabD50
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
