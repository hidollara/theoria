use super::{ColorProfile, ConnectionSpace, SpaceConnectionError, TypedColorSpace};
use crate::model::Oklab as OklabModel;
use crate::transfer::{Transfer, TransferCurve, TransferError, TransferMatrix, TransferStage};

named_space!(OklabD65, OklabModel);

impl OklabD65 {
    const XYZ_D65_TO_LMS: [[f64; 3]; 3] = [
        [0.819022437996703, 0.3619062600528904, -0.1288737815209879],
        [0.0329836539323885, 0.9292868615863434, 0.0361446663506424],
        [0.0481771893596242, 0.2642395317527308, 0.6335478284694309],
    ];

    const CBRT_LMS_TO_OKLAB: [[f64; 3]; 3] = [
        [0.210454268309314, 0.7936177747023054, -0.0040720430116193],
        [1.9779985324311684, -2.42859224204858, 0.450593709617411],
        [0.0259040424655478, 0.7827717124575296, -0.8086757549230774],
    ];

    const OKLAB_TO_CBRT_LMS: [[f64; 3]; 3] = [
        [1.0, 0.3963377773761749, 0.2158037573099136],
        [1.0, -0.1055613458156586, -0.0638541728258133],
        [1.0, -0.0894841775298119, -1.2914855480194092],
    ];

    const LMS_TO_XYZ_D65: [[f64; 3]; 3] = [
        [1.2268798758459243, -0.5578149944602171, 0.2813910456659647],
        [-0.0405757452148008, 1.112286803280317, -0.0717110580655164],
        [-0.0763729366746601, -0.4214933324022432, 1.5869240198367816],
    ];

    pub(crate) fn transfer_from_xyz_d65() -> Result<Transfer, TransferError> {
        Transfer::new([
            TransferStage::Matrix(TransferMatrix::new(Self::XYZ_D65_TO_LMS, [0.0; 3])?),
            TransferStage::curves(vec![TransferCurve::signed_power(1.0 / 3.0)?; 3])?,
            TransferStage::Matrix(TransferMatrix::new(Self::CBRT_LMS_TO_OKLAB, [0.0; 3])?),
        ])
    }

    pub(crate) fn transfer_to_xyz_d65() -> Result<Transfer, TransferError> {
        Transfer::new([
            TransferStage::Matrix(TransferMatrix::new(Self::OKLAB_TO_CBRT_LMS, [0.0; 3])?),
            TransferStage::curves(vec![TransferCurve::signed_power(3.0)?; 3])?,
            TransferStage::Matrix(TransferMatrix::new(Self::LMS_TO_XYZ_D65, [0.0; 3])?),
        ])
    }
}

builtin_space!(OklabD65, OklabD65Profile);

impl ColorProfile for OklabD65Profile {
    fn channels(&self) -> usize {
        3
    }

    fn connection_space(&self) -> ConnectionSpace {
        ConnectionSpace::XyzD65
    }

    fn transfer_to_connection_space(&self) -> Result<Transfer, SpaceConnectionError> {
        Ok(OklabD65::transfer_to_xyz_d65()?)
    }

    fn transfer_from_connection_space(&self) -> Result<Transfer, SpaceConnectionError> {
        Ok(OklabD65::transfer_from_xyz_d65()?)
    }
}
