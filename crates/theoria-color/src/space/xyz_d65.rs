use super::{
    ColorProfile, ColorSpace, ColorTransform, IccPcs, IccRenderingIntent, ProfileConnectionError,
    ProfileDefinition, TypedColorSpace, XyzD50, require_relative,
};
use crate::color_transform::adaptation;
use crate::model::Xyz;
use crate::profile_data::{XYZ_D50_TO_XYZ_D65, XYZ_D65_TO_XYZ_D50};

named_space!(XyzD65, Xyz);
predefined_space!(XyzD65, XYZ_D65_KEY);

impl ProfileDefinition for XyzD65 {
    fn space(&self) -> ColorSpace {
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
        Ok(ColorTransform::new(
            self.space(),
            XyzD50.into(),
            adaptation(XYZ_D65_TO_XYZ_D50)?,
        )?)
    }

    fn pcs_to_space(
        &self,
        intent: IccRenderingIntent,
    ) -> Result<ColorTransform, ProfileConnectionError> {
        require_relative(intent)?;
        Ok(ColorTransform::new(
            XyzD50.into(),
            self.space(),
            adaptation(XYZ_D50_TO_XYZ_D65)?,
        )?)
    }
}
