mod connection;
mod icc;
mod model;
mod profile;
pub(crate) mod profile_data;
mod rgb;
mod sample;
mod transfer;
mod transform;
mod value;
mod white_point;

pub use connection::XyzD50Profile;
pub use icc::{IccPcs, IccTransfer, IccTransferDirection, IccTransferError};
pub use model::{
    Cmyk, ColorCoordinates, ColorModel, DeviceN, Lab, ReferenceSample, Rgb, Xyz, Ycbcr,
};
pub use profile::{
    ColorProfile, DisplayP3, LabD50, LinearDisplayP3, LinearSrgb, MatrixRgbProfile, RgbColorSpace,
    Srgb, XyzD50, XyzD65,
};
pub use sample::{Opacity, Sample, UnitF32, UnitF64, UnitSample, UnitSampleError};
pub use transfer::{
    Transfer, TransferClut, TransferCurve, TransferError, TransferMatrix, TransferShader,
    TransferStage,
};
pub use transform::{RgbTransfer, RgbTransform};
pub use value::{Color, ColorError};
pub use white_point::{WhitePoint, WhitePointError};

pub(crate) use rgb::{convert_rgb, encode_srgb, from_linear_srgb, to_linear_srgb};
