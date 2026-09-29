//! Color models, spaces, profiles, and directed transfers.
//!
//! In Theoria, a [`ColorProfile`] fixes a coordinate model and its directed
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
pub use model::{Cmyk, ColorCoordinates, ColorModel, Lab, Rgb, TypedColorModel, Xyz, Ycbcr};
pub use space::{
    ColorProfile, ColorSpace, ConnectionSpace, DisplayP3, LabD50, LinearDisplayP3, LinearSrgb,
    SpaceConnectionError, Srgb, TypedColorSpace, XyzD50, XyzD65,
};
pub use transfer::{
    CurveSegment, CurveSegmentKind, Transfer, TransferClut, TransferCurve, TransferCurveKind,
    TransferError, TransferMatrix, TransferStage,
};
