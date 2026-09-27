mod color_transform;
mod connection;
mod icc;
mod icc_mpe;
mod icc_profile;
mod model;
mod pcs;
mod profile;
mod rgb;
mod sample;
mod space;
mod transfer;
mod transform;
mod value;
mod white_point;

pub use color_transform::{ColorTransform, ColorTransformError};
pub use connection::XyzD50Profile;
pub use icc::{IccPcs, IccTransfer, IccTransferDirection, IccTransferError};
pub use icc_profile::{IccProfile, IccProfileError, IccRenderingIntent};
pub use model::{Cmyk, ColorCoordinates, ColorModel, Lab, Rgb, Xyz, Ycbcr};
pub use profile::{ColorProfile, ProfileConnectionError, ProfileDefinition, connect_profiles};
pub use sample::{Opacity, OpacityError, OpacitySample};
pub use space::{
    ColorSpace, DisplayP3, LabD50, LinearDisplayP3, LinearSrgb, RgbColorSpace, Srgb,
    TypedColorSpace, XyzD50, XyzD65,
};
pub use transfer::{
    Transfer, TransferClut, TransferCurve, TransferError, TransferMatrix, TransferShader,
    TransferStage,
};
pub(crate) use transform::RgbTransfer;
pub use value::Color;
pub use white_point::{WhitePoint, WhitePointError};

#[doc(hidden)]
pub use rgb::{convert_rgb, encode_srgb};
