use super::{ColorProfile, ConnectionSpace, SpaceConnectionError, TypedColorSpace};
use crate::model::Rgb;
use crate::transfer::{Transfer, TransferCurve, TransferError, TransferMatrix, TransferStage};

named_space!(A98Rgb, Rgb);

impl A98Rgb {
    const LINEAR_RGB_TO_XYZ_D65: [[f64; 3]; 3] = [
        [
            573536.0 / 994567.0,
            263643.0 / 1420810.0,
            187206.0 / 994567.0,
        ],
        [
            591459.0 / 1989134.0,
            6239551.0 / 9945670.0,
            374412.0 / 4972835.0,
        ],
        [
            53769.0 / 1989134.0,
            351524.0 / 4972835.0,
            4929758.0 / 4972835.0,
        ],
    ];

    const XYZ_D65_TO_LINEAR_RGB: [[f64; 3]; 3] = [
        [
            1829569.0 / 896150.0,
            -506331.0 / 896150.0,
            -308931.0 / 896150.0,
        ],
        [
            -851781.0 / 878810.0,
            1648619.0 / 878810.0,
            36519.0 / 878810.0,
        ],
        [
            16779.0 / 1248040.0,
            -147721.0 / 1248040.0,
            1266979.0 / 1248040.0,
        ],
    ];

    pub(crate) fn transfer_to_xyz_d65() -> Result<Transfer, TransferError> {
        Transfer::new([
            TransferStage::curves(vec![TransferCurve::signed_power(563.0 / 256.0)?; 3])?,
            TransferStage::Matrix(TransferMatrix::new(Self::LINEAR_RGB_TO_XYZ_D65, [0.0; 3])?),
        ])
    }

    pub(crate) fn transfer_from_xyz_d65() -> Result<Transfer, TransferError> {
        Transfer::new([
            TransferStage::Matrix(TransferMatrix::new(Self::XYZ_D65_TO_LINEAR_RGB, [0.0; 3])?),
            TransferStage::curves(vec![TransferCurve::signed_power(256.0 / 563.0)?; 3])?,
        ])
    }
}

builtin_space!(A98Rgb, A98RgbProfile);

impl ColorProfile for A98RgbProfile {
    fn channels(&self) -> usize {
        3
    }

    fn connection_space(&self) -> ConnectionSpace {
        ConnectionSpace::XyzD65
    }

    fn transfer_to_connection_space(&self) -> Result<Transfer, SpaceConnectionError> {
        Ok(A98Rgb::transfer_to_xyz_d65()?)
    }

    fn transfer_from_connection_space(&self) -> Result<Transfer, SpaceConnectionError> {
        Ok(A98Rgb::transfer_from_xyz_d65()?)
    }
}
