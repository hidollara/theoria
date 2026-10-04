//! Color models, spaces, profiles, and directed transfers.
//!
//! In Theoria, a [`ColorProfile`] fixes the channel count and directed
//! connection-space conversions. A [`ColorSpace`] is the shared handle and coordinate
//! system formed from that profile. An ICC [`IccProfile`] can contain several
//! rendering-intent transforms; selecting one intent forms a `ColorProfile`
//! and a corresponding `ColorSpace`. The ICC data alone is not a single space.

mod icc_mpe;
mod icc_profile;
mod model;
mod space;
mod transfer;

pub use icc_profile::{IccProfile, IccProfileError, IccRenderingIntent};
pub use model::{ColorModel, Hsl, Hwb, Lab, Lch, Oklab, Oklch, Rgb, TypedColorModel, Xyz};
pub use space::{
    A98Rgb, ColorProfile, ColorSpace, ConnectionSpace, DisplayP3, HslSrgb, HwbSrgb, LabD50, LchD50,
    LinearDisplayP3, LinearSrgb, OklabD65, OklchD65, ProPhotoRgb, Rec2020, SpaceConnectionError,
    Srgb, TypedColorSpace, XyzD50, XyzD65,
};
pub use transfer::{
    ColorFunction, CurveSegment, CurveSegmentKind, Transfer, TransferClut, TransferCurve,
    TransferCurveKind, TransferError, TransferMatrix, TransferStage,
};
