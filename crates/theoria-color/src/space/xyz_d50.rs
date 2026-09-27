use super::{
    ColorProfile, ColorSpace, ColorSpaceKey, ColorTransform, IccPcs, IccRenderingIntent,
    ProfileConnectionError, require_relative,
};
use crate::model::Xyz;

named_space!(XyzD50, Xyz);
predefined_space!(XyzD50, XYZ_D50_KEY);

impl ColorProfile for XyzD50 {
    fn space(&self) -> ColorSpaceKey {
        (*self).into()
    }

    fn pcs(&self) -> IccPcs {
        IccPcs::XyzD50
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
