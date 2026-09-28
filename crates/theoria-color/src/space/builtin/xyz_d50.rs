use super::{ColorProfile, Pcs, SpaceConnectionError, Transfer, TypedColorSpace};
use crate::model::Xyz;

named_space!(XyzD50, Xyz);
builtin_space!(XyzD50, XyzD50Profile);

impl ColorProfile for XyzD50Profile {
    fn model(&self) -> crate::model::ColorModel {
        crate::model::ColorModel::Xyz
    }

    fn pcs(&self) -> Pcs {
        Pcs::XyzD50
    }

    fn space_to_pcs_transfer(&self) -> Result<Transfer, SpaceConnectionError> {
        Ok(Transfer::new(3, [])?)
    }

    fn pcs_to_space_transfer(&self) -> Result<Transfer, SpaceConnectionError> {
        self.space_to_pcs_transfer()
    }
}
