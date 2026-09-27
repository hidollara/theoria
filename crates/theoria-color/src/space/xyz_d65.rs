use super::{
    ColorProfile, ColorSpace, ColorTransform, IccRenderingIntent, Pcs, ProfileConnectionError,
    ProfileDefinition, TypedColorSpace, XyzD50, require_relative,
};
use crate::color_transform::adaptation;
use crate::model::Xyz;

named_space!(XyzD65, Xyz);
predefined_space!(XyzD65, XYZ_D65_KEY);

impl XyzD65 {
    // Linear Bradford adaptation from the CSS Color 4 conversion sample.
    // https://www.w3.org/TR/css-color-4/#color-conversion-code
    pub(crate) const TO_XYZ_D50: [[f64; 3]; 3] = [
        [
            1.0479297925449969,
            0.022946870601609652,
            -0.05019226628920524,
        ],
        [
            0.02962780877005599,
            0.9904344267538799,
            -0.017073799063418826,
        ],
        [
            -0.009243040646204504,
            0.015055191490298152,
            0.7518742814281371,
        ],
    ];
    pub(crate) const FROM_XYZ_D50: [[f64; 3]; 3] = [
        [0.955473421488075, -0.02309845494876471, 0.06325924320057072],
        [
            -0.0283697093338637,
            1.0099953980813041,
            0.021041441191917323,
        ],
        [
            0.012314014864481998,
            -0.020507649298898964,
            1.330365926242124,
        ],
    ];
}

impl ProfileDefinition for XyzD65 {
    fn space(&self) -> ColorSpace {
        (*self).into()
    }

    fn pcs(&self) -> Pcs {
        Pcs::XyzD50
    }

    fn space_to_pcs(
        &self,
        intent: IccRenderingIntent,
    ) -> Result<ColorTransform, ProfileConnectionError> {
        require_relative(intent)?;
        Ok(ColorTransform::new(
            self.space(),
            XyzD50.into(),
            adaptation(Self::TO_XYZ_D50)?,
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
            adaptation(Self::FROM_XYZ_D50)?,
        )?)
    }
}
