use super::{
    ColorSpace, ColorSpaceId, ColorTransform, IccRenderingIntent, Pcs, ProfileConnectionError,
    ProfileDefinition, TypedColorSpace, require_relative,
};
use crate::model::Xyz;

named_space!(XyzD50, Xyz);
predefined_space!(XyzD50, XyzD50Profile, XYZ_D50_KEY);

impl ProfileDefinition for XyzD50Profile {
    fn space(&self) -> ColorSpaceId {
        XyzD50.into()
    }

    fn pcs(&self) -> Pcs {
        Pcs::XyzD50
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
