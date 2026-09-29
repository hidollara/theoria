use std::{
    error::Error,
    fmt,
    sync::{Arc, OnceLock},
};

use moxcms::{
    ColorProfile as ParsedProfile, DataColorSpace, LutDataType, LutMultidimensionalType, LutStore,
    LutType, LutWarehouse, ProfileClass, ProfileVersion, ToneReprCurve,
};

use super::icc_mpe::MpeTags;
use super::model::ColorModel;
use super::space::{
    ColorProfile, ColorSpace, ConnectionSpace, LabD50, SpaceConnectionError, XyzD50,
};
use super::transfer::{
    CurveSegment, CurveSegmentKind, ICC_PCS_D50_XYZ, Transfer, TransferClut, TransferCurve,
    TransferError, TransferMatrix, TransferStage,
};

/// The two PCS encodings permitted by the supported ICC profiles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pcs {
    XyzD50,
    LabD50,
}

impl From<Pcs> for ConnectionSpace {
    fn from(pcs: Pcs) -> Self {
        match pcs {
            Pcs::XyzD50 => Self::XyzD50,
            Pcs::LabD50 => Self::LabD50,
        }
    }
}

/// The requested rendering intent for selecting a profile conversion.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IccRenderingIntent {
    Perceptual,
    RelativeColorimetric,
    Saturation,
    AbsoluteColorimetric,
}

/// A parsed ICC v2/v4 profile whose supported transforms can be lowered to
/// [`Transfer`] for both CPU evaluation and WGSL generation.
///
/// A profile is not itself a single transform: direction and rendering intent
/// select an MPE, a LUT, or a matrix/TRC model. When a direct DToB3/BToD3
/// MPE is absent, absolute colorimetry applies the ICC media-white adjustment
/// to the relative-colorimetric model. PCS values
/// exposed by its transfers are actual D50 XYZ or Lab coordinates, not the
/// normalized integer encodings used inside LUT tags.
#[derive(Clone, Debug)]
pub struct IccProfile {
    inner: Arc<IccProfileData>,
    // Separate from `inner` so selected spaces can own the data without a cycle.
    // Clones share these slots and therefore reuse one space per intent.
    selected: Arc<[OnceLock<ColorSpace>; 4]>,
}

struct IccColorProfile {
    data: Arc<IccProfileData>,
    intent: IccRenderingIntent,
}

#[derive(Clone, Debug)]
struct IccProfileData {
    parsed: Arc<ParsedProfile>,
    original_bytes: Option<Arc<[u8]>>,
    pcs: Pcs,
    mpe: MpeTags,
}

impl IccProfile {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, IccProfileError> {
        let parsed = ParsedProfile::new_from_slice(bytes).map_err(IccProfileError::Parse)?;
        if !matches!(
            parsed.version(),
            ProfileVersion::V2_0
                | ProfileVersion::V2_1
                | ProfileVersion::V2_2
                | ProfileVersion::V2_3
                | ProfileVersion::V2_4
                | ProfileVersion::V4_0
                | ProfileVersion::V4_1
                | ProfileVersion::V4_2
                | ProfileVersion::V4_3
                | ProfileVersion::V4_4
        ) {
            return Err(IccProfileError::UnsupportedVersion);
        }
        if !matches!(
            parsed.profile_class,
            ProfileClass::InputDevice
                | ProfileClass::DisplayDevice
                | ProfileClass::OutputDevice
                | ProfileClass::ColorSpace
        ) {
            return Err(IccProfileError::UnsupportedProfileClass);
        }
        let pcs = match parsed.pcs {
            DataColorSpace::Xyz => Pcs::XyzD50,
            DataColorSpace::Lab => Pcs::LabD50,
            _ => return Err(IccProfileError::UnsupportedPcs),
        };
        let mpe = MpeTags::parse(bytes).map_err(|_| IccProfileError::MalformedTransfer)?;
        let mut profile = Self::from_parsed(parsed, pcs);
        let data = Arc::get_mut(&mut profile.inner).expect("new profile has one owner");
        data.original_bytes = Some(Arc::from(bytes));
        data.mpe = mpe;
        Ok(profile)
    }

    fn from_parsed(parsed: ParsedProfile, pcs: Pcs) -> Self {
        Self {
            inner: Arc::new(IccProfileData {
                parsed: Arc::new(parsed),
                original_bytes: None,
                pcs,
                mpe: MpeTags::default(),
            }),
            selected: Arc::new(std::array::from_fn(|_| OnceLock::new())),
        }
    }

    pub fn device_channels(&self) -> usize {
        self.inner.device_channels()
    }

    /// Returns the exact loaded ICC payload for embedding in an encoded image.
    /// Profiles synthesized internally for tests do not have source bytes.
    pub fn original_bytes(&self) -> Option<&[u8]> {
        self.inner.original_bytes.as_deref()
    }

    /// Selects one rendering intent as a Theoria color profile and returns its
    /// shared color-space handle. The ICC data remains reusable for other intents.
    pub fn color_space(&self, intent: IccRenderingIntent) -> ColorSpace {
        self.selected[intent.index()]
            .get_or_init(|| {
                ColorSpace::new(IccColorProfile {
                    data: self.inner.clone(),
                    intent,
                })
            })
            .clone()
    }
}

impl IccProfileData {
    fn device_channels(&self) -> usize {
        self.device_model().channels()
    }

    fn device_model(&self) -> ColorModel {
        color_model(self.parsed.color_space)
    }

    fn space_to_pcs(&self, intent: IccRenderingIntent) -> Result<Transfer, IccProfileError> {
        if let Some(transfer) = &self.mpe.to_pcs[intent.index()] {
            return self.validate_to_pcs(transfer.clone());
        }
        if intent == IccRenderingIntent::AbsoluteColorimetric {
            return self.absolute_to_pcs();
        }
        // ICC.1 section 8.10: selected DToB, selected AToB, AToB0, then matrix/TRC.
        let tag = match intent {
            IccRenderingIntent::Perceptual => &self.parsed.lut_a_to_b_perceptual,
            IccRenderingIntent::RelativeColorimetric => &self.parsed.lut_a_to_b_colorimetric,
            IccRenderingIntent::Saturation => &self.parsed.lut_a_to_b_saturation,
            IccRenderingIntent::AbsoluteColorimetric => unreachable!(),
        }
        .as_ref()
        .or(self.parsed.lut_a_to_b_perceptual.as_ref());
        let transfer = if let Some(tag) = tag {
            let mut stages = match tag {
                LutWarehouse::Lut(lut) => legacy_lut(lut, self.pcs)?,
                LutWarehouse::Multidimensional(lut) => multidimensional_lut_to_pcs(lut)?,
            };
            stages.push(pcs_decoding(self.pcs, tag)?);
            Transfer::new(stages)?
        } else if self.parsed.color_space == DataColorSpace::Gray {
            gray_trc_to_pcs(&self.parsed, self.pcs)?
        } else if self.parsed.color_space == DataColorSpace::Rgb && self.pcs == Pcs::XyzD50 {
            rgb_matrix_trc_to_pcs(&self.parsed)?
        } else {
            return Err(IccProfileError::MissingTransfer);
        };
        self.validate_to_pcs(transfer)
    }

    fn pcs_to_space(&self, intent: IccRenderingIntent) -> Result<Transfer, IccProfileError> {
        if let Some(transfer) = &self.mpe.from_pcs[intent.index()] {
            return self.validate_from_pcs(transfer.clone());
        }
        if intent == IccRenderingIntent::AbsoluteColorimetric {
            return self.absolute_from_pcs();
        }
        // ICC.1 section 8.10: selected BToD, selected BToA, BToA0, then matrix/TRC.
        let tag = match intent {
            IccRenderingIntent::Perceptual => &self.parsed.lut_b_to_a_perceptual,
            IccRenderingIntent::RelativeColorimetric => &self.parsed.lut_b_to_a_colorimetric,
            IccRenderingIntent::Saturation => &self.parsed.lut_b_to_a_saturation,
            IccRenderingIntent::AbsoluteColorimetric => unreachable!(),
        }
        .as_ref()
        .or(self.parsed.lut_b_to_a_perceptual.as_ref());
        let transfer = if let Some(tag) = tag {
            let mut stages = vec![pcs_encoding(self.pcs, tag)?, TransferStage::Clamp(3)];
            stages.extend(match tag {
                LutWarehouse::Lut(lut) => legacy_lut_from_pcs(lut, self.pcs)?,
                LutWarehouse::Multidimensional(lut) => multidimensional_lut_from_pcs(lut)?,
            });
            Transfer::new(stages)?
        } else if self.parsed.color_space == DataColorSpace::Gray {
            gray_trc_from_pcs(&self.parsed, self.pcs)?
        } else if self.parsed.color_space == DataColorSpace::Rgb && self.pcs == Pcs::XyzD50 {
            rgb_matrix_trc_from_pcs(&self.parsed)?
        } else {
            return Err(IccProfileError::MissingTransfer);
        };
        self.validate_from_pcs(transfer)
    }

    fn validate_to_pcs(&self, transfer: Transfer) -> Result<Transfer, IccProfileError> {
        if !transfer.supports_channels(self.device_channels(), 3) {
            return Err(IccProfileError::MalformedTransfer);
        }
        Ok(transfer)
    }

    fn validate_from_pcs(&self, transfer: Transfer) -> Result<Transfer, IccProfileError> {
        if !transfer.supports_channels(3, self.device_channels()) {
            return Err(IccProfileError::MalformedTransfer);
        }
        Ok(transfer)
    }

    fn media_white(&self) -> Result<[f64; 3], IccProfileError> {
        let media_white = self
            .parsed
            .media_white_point
            .ok_or(IccProfileError::MissingMediaWhitePoint)?;
        let media_white = [media_white.x, media_white.y, media_white.z];
        if media_white
            .iter()
            .any(|value| !value.is_finite() || *value <= 0.0)
        {
            return Err(IccProfileError::MalformedTransfer);
        }
        Ok(media_white)
    }

    fn absolute_adjustment(&self, scale: [f64; 3]) -> Result<Transfer, IccProfileError> {
        let adjustment = Transfer::new([diagonal(scale, [0.0; 3])?])?;
        if self.pcs == Pcs::LabD50 {
            Ok(LabD50::to_xyz_transfer()
                .then(&adjustment)?
                .then(&XyzD50::to_lab_transfer())?)
        } else {
            Ok(adjustment)
        }
    }

    fn absolute_to_pcs(&self) -> Result<Transfer, IccProfileError> {
        let media_white = self.media_white()?;
        let scale = std::array::from_fn(|index| media_white[index] / ICC_PCS_D50_XYZ[index]);
        let relative = self.space_to_pcs(IccRenderingIntent::RelativeColorimetric)?;
        self.validate_to_pcs(relative.then(&self.absolute_adjustment(scale)?)?)
    }

    fn absolute_from_pcs(&self) -> Result<Transfer, IccProfileError> {
        let media_white = self.media_white()?;
        let scale = std::array::from_fn(|index| ICC_PCS_D50_XYZ[index] / media_white[index]);
        let relative = self.pcs_to_space(IccRenderingIntent::RelativeColorimetric)?;
        self.validate_from_pcs(self.absolute_adjustment(scale)?.then(&relative)?)
    }
}

impl IccRenderingIntent {
    const fn index(self) -> usize {
        match self {
            Self::Perceptual => 0,
            Self::RelativeColorimetric => 1,
            Self::Saturation => 2,
            Self::AbsoluteColorimetric => 3,
        }
    }
}

fn color_model(space: DataColorSpace) -> ColorModel {
    match space {
        DataColorSpace::Xyz => ColorModel::Xyz,
        DataColorSpace::Lab => ColorModel::Lab,
        DataColorSpace::Luv => ColorModel::Luv,
        DataColorSpace::YCbr => ColorModel::Ycbcr,
        DataColorSpace::Yxy => ColorModel::Yxy,
        DataColorSpace::Rgb => ColorModel::Rgb,
        DataColorSpace::Gray => ColorModel::Gray,
        DataColorSpace::Hsv => ColorModel::Hsv,
        DataColorSpace::Hls => ColorModel::Hls,
        DataColorSpace::Cmyk => ColorModel::Cmyk,
        DataColorSpace::Cmy => ColorModel::Cmy,
        DataColorSpace::Color2 => ColorModel::Channels(2),
        DataColorSpace::Color3 => ColorModel::Channels(3),
        DataColorSpace::Color4 => ColorModel::Channels(4),
        DataColorSpace::Color5 => ColorModel::Channels(5),
        DataColorSpace::Color6 => ColorModel::Channels(6),
        DataColorSpace::Color7 => ColorModel::Channels(7),
        DataColorSpace::Color8 => ColorModel::Channels(8),
        DataColorSpace::Color9 => ColorModel::Channels(9),
        DataColorSpace::Color10 => ColorModel::Channels(10),
        DataColorSpace::Color11 => ColorModel::Channels(11),
        DataColorSpace::Color12 => ColorModel::Channels(12),
        DataColorSpace::Color13 => ColorModel::Channels(13),
        DataColorSpace::Color14 => ColorModel::Channels(14),
        DataColorSpace::Color15 => ColorModel::Channels(15),
    }
}

fn curve(curve: &ToneReprCurve) -> Result<TransferCurve, IccProfileError> {
    Ok(match curve {
        ToneReprCurve::Lut(values) if values.is_empty() => TransferCurve::IDENTITY,
        ToneReprCurve::Lut(values) if values.len() == 1 => {
            TransferCurve::gamma(f64::from(values[0]) / 256.0)?
        }
        ToneReprCurve::Lut(values) => TransferCurve::sampled(
            values
                .iter()
                .map(|value| f64::from(*value) / 65535.0)
                .collect::<Vec<_>>(),
        )?,
        ToneReprCurve::Parametric(values) => parametric_curve(
            &values
                .iter()
                .map(|value| f64::from(*value))
                .collect::<Vec<_>>(),
        )?,
    })
}

fn parametric_curve(parameters: &[f64]) -> Result<TransferCurve, IccProfileError> {
    let function = match parameters.len() {
        1 => 0,
        3 => 1,
        4 => 2,
        5 => 3,
        7 => 4,
        _ => return Err(IccProfileError::MalformedTransfer),
    };
    let p = parameters;
    if p.iter().any(|value| !value.is_finite())
        || p[0] <= 0.0
        || (function == 1 || function == 2) && p[1] <= 0.0
        || (function == 3 || function == 4)
            && (p[1] < 0.0 || (p[4] <= 1.0 && p[1] * p[4].max(0.0) + p[2] < 0.0))
    {
        return Err(TransferError::InvalidCurve.into());
    }
    if function == 0 {
        return Ok(TransferCurve::gamma(p[0])?);
    }
    let output_offset = if function == 2 {
        p[3]
    } else if function == 4 {
        p[5]
    } else {
        0.0
    };
    let high = CurveSegmentKind::PowerAffine {
        scale: p[1],
        offset: p[2],
        exponent: p[0],
        output_scale: 1.0,
        output_offset,
    };
    let (breakpoint, low) = if function == 1 || function == 2 {
        (-p[2] / p[1], CurveSegmentKind::Constant(output_offset))
    } else {
        (
            p[4],
            CurveSegmentKind::Linear {
                scale: p[3],
                offset: if function == 4 { p[6] } else { 0.0 },
            },
        )
    };
    let curve = TransferCurve::piecewise([
        CurveSegment::exclusive(breakpoint, low),
        CurveSegment::new(f64::INFINITY, high),
    ])?;
    Ok(TransferCurve::clamped(curve, [0.0, 1.0], [0.0, 1.0])?)
}

fn inverse_curve(value: &ToneReprCurve) -> Result<TransferCurve, IccProfileError> {
    match value {
        ToneReprCurve::Lut(values) if values.is_empty() => Ok(TransferCurve::IDENTITY),
        ToneReprCurve::Lut(values) if values.len() == 1 => {
            Ok(TransferCurve::gamma(256.0 / f64::from(values[0]))?)
        }
        _ => curve(&value.inverse().map_err(IccProfileError::Parse)?),
    }
}

fn curves(values: &[ToneReprCurve], channels: usize) -> Result<TransferStage, IccProfileError> {
    if values.len() != channels {
        return Err(IccProfileError::MalformedTransfer);
    }
    Ok(TransferStage::curves(
        values.iter().map(curve).collect::<Result<Vec<_>, _>>()?,
    )?)
}

fn matrix(value: &moxcms::Matrix3d, bias: [f64; 3]) -> Result<TransferStage, IccProfileError> {
    Ok(TransferStage::Matrix(TransferMatrix::new(
        3,
        3,
        value.v.into_iter().flatten().collect::<Vec<_>>(),
        bias,
    )?))
}

fn normalized_values(store: &LutStore) -> Vec<f64> {
    match store {
        LutStore::Store8(values) => values
            .iter()
            .map(|value| f64::from(*value) / 255.0)
            .collect(),
        LutStore::Store16(values) => values
            .iter()
            .map(|value| f64::from(*value) / 65535.0)
            .collect(),
    }
}

fn clut(grid: &[u8], outputs: usize, store: &LutStore) -> Result<TransferStage, IccProfileError> {
    Ok(TransferStage::Clut(TransferClut::new(
        grid.to_vec(),
        outputs,
        normalized_values(store),
    )?))
}

fn table_curves(
    store: &LutStore,
    channels: usize,
    entries: usize,
) -> Result<TransferStage, IccProfileError> {
    let values = normalized_values(store);
    if entries < 2 || values.len() != channels * entries {
        return Err(IccProfileError::MalformedTransfer);
    }
    Ok(TransferStage::curves(
        values
            .chunks_exact(entries)
            .map(|samples| TransferCurve::sampled(samples.to_vec()))
            .collect::<Result<Vec<_>, _>>()?,
    )?)
}

fn legacy_lut(lut: &LutDataType, pcs: Pcs) -> Result<Vec<TransferStage>, IccProfileError> {
    // ICC does not define an 8-bit PCSXYZ encoding for lut8Type.
    if lut.lut_type == LutType::Lut8 && pcs == Pcs::XyzD50 {
        return Err(IccProfileError::MalformedTransfer);
    }
    let inputs = usize::from(lut.num_input_channels);
    let outputs = usize::from(lut.num_output_channels);
    Ok(vec![
        table_curves(
            &lut.input_table,
            inputs,
            usize::from(lut.num_input_table_entries),
        )?,
        clut(
            &vec![lut.num_clut_grid_points; inputs],
            outputs,
            &lut.clut_table,
        )?,
        table_curves(
            &lut.output_table,
            outputs,
            usize::from(lut.num_output_table_entries),
        )?,
    ])
}

fn legacy_lut_from_pcs(lut: &LutDataType, pcs: Pcs) -> Result<Vec<TransferStage>, IccProfileError> {
    let mut stages = Vec::new();
    // The legacy LUT matrix is only meaningful for a three-channel PCSXYZ
    // input; conforming tags use identity in all other cases.
    if pcs == Pcs::XyzD50 && lut.num_input_channels == 3 {
        stages.push(matrix(&lut.matrix, [0.0; 3])?);
    }
    stages.extend(legacy_lut(lut, pcs)?);
    Ok(stages)
}

fn validate_multidimensional_lut(lut: &LutMultidimensionalType) -> Result<(), IccProfileError> {
    if lut.b_curves.len() != 3
        || lut.a_curves.is_empty() != lut.clut.is_none()
        || !lut.m_curves.is_empty() && lut.m_curves.len() != 3
    {
        return Err(IccProfileError::MalformedTransfer);
    }
    Ok(())
}

fn multidimensional_lut_to_pcs(
    lut: &LutMultidimensionalType,
) -> Result<Vec<TransferStage>, IccProfileError> {
    validate_multidimensional_lut(lut)?;
    let inputs = usize::from(lut.num_input_channels);
    let outputs = usize::from(lut.num_output_channels);
    if outputs != 3 {
        return Err(IccProfileError::MalformedTransfer);
    }
    let mut stages = Vec::new();
    if !lut.a_curves.is_empty() {
        stages.push(curves(&lut.a_curves, inputs)?);
    }
    if let Some(table) = &lut.clut {
        stages.push(clut(&lut.grid_points[..inputs], outputs, table)?);
    }
    if !lut.m_curves.is_empty() {
        stages.push(curves(&lut.m_curves, 3)?);
        stages.push(matrix(&lut.matrix, lut.bias.v)?);
        stages.push(TransferStage::Clamp(3));
    }
    stages.push(curves(&lut.b_curves, 3)?);
    Ok(stages)
}

fn multidimensional_lut_from_pcs(
    lut: &LutMultidimensionalType,
) -> Result<Vec<TransferStage>, IccProfileError> {
    validate_multidimensional_lut(lut)?;
    let inputs = usize::from(lut.num_input_channels);
    let outputs = usize::from(lut.num_output_channels);
    if inputs != 3 {
        return Err(IccProfileError::MalformedTransfer);
    }
    let mut stages = vec![curves(&lut.b_curves, 3)?];
    if !lut.m_curves.is_empty() {
        stages.push(matrix(&lut.matrix, lut.bias.v)?);
        stages.push(TransferStage::Clamp(3));
        stages.push(curves(&lut.m_curves, 3)?);
    }
    if let Some(table) = &lut.clut {
        stages.push(clut(&lut.grid_points[..3], outputs, table)?);
    }
    if !lut.a_curves.is_empty() {
        stages.push(curves(&lut.a_curves, outputs)?);
    }
    Ok(stages)
}

fn pcs_decoding(pcs: Pcs, tag: &LutWarehouse) -> Result<TransferStage, IccProfileError> {
    let legacy_lab = matches!(tag, LutWarehouse::Lut(lut) if lut.lut_type == LutType::Lut16);
    let (scale, offsets) = match pcs {
        Pcs::XyzD50 => ([65535.0 / 32768.0; 3], [0.0; 3]),
        Pcs::LabD50 if legacy_lab => (
            [100.0 * 65535.0 / 65280.0, 65535.0 / 256.0, 65535.0 / 256.0],
            [0.0, -128.0, -128.0],
        ),
        Pcs::LabD50 => ([100.0, 255.0, 255.0], [0.0, -128.0, -128.0]),
    };
    diagonal(scale, offsets)
}

fn pcs_encoding(pcs: Pcs, tag: &LutWarehouse) -> Result<TransferStage, IccProfileError> {
    let legacy_lab = matches!(tag, LutWarehouse::Lut(lut) if lut.lut_type == LutType::Lut16);
    let (scale, offsets) = match pcs {
        Pcs::XyzD50 => ([32768.0 / 65535.0; 3], [0.0; 3]),
        Pcs::LabD50 if legacy_lab => (
            [
                65280.0 / (100.0 * 65535.0),
                256.0 / 65535.0,
                256.0 / 65535.0,
            ],
            [0.0, 128.0 * 256.0 / 65535.0, 128.0 * 256.0 / 65535.0],
        ),
        Pcs::LabD50 => (
            [0.01, 1.0 / 255.0, 1.0 / 255.0],
            [0.0, 128.0 / 255.0, 128.0 / 255.0],
        ),
    };
    diagonal(scale, offsets)
}

fn diagonal(scale: [f64; 3], offsets: [f64; 3]) -> Result<TransferStage, IccProfileError> {
    Ok(TransferStage::Matrix(TransferMatrix::new(
        3,
        3,
        [scale[0], 0.0, 0.0, 0.0, scale[1], 0.0, 0.0, 0.0, scale[2]],
        offsets,
    )?))
}

fn rgb_trcs(profile: &ParsedProfile) -> Result<[&ToneReprCurve; 3], IccProfileError> {
    Ok([
        profile
            .red_trc
            .as_ref()
            .ok_or(IccProfileError::MissingTransfer)?,
        profile
            .green_trc
            .as_ref()
            .ok_or(IccProfileError::MissingTransfer)?,
        profile
            .blue_trc
            .as_ref()
            .ok_or(IccProfileError::MissingTransfer)?,
    ])
}

fn rgb_matrix_trc_to_pcs(profile: &ParsedProfile) -> Result<Transfer, IccProfileError> {
    let trcs = rgb_trcs(profile)?;
    let matrix_value = profile.rgb_to_xyz_matrix();
    Ok(Transfer::new([
        TransferStage::curves(
            trcs.iter()
                .map(|value| curve(value))
                .collect::<Result<Vec<_>, _>>()?,
        )?,
        matrix(&matrix_value, [0.0; 3])?,
    ])?)
}

fn rgb_matrix_trc_from_pcs(profile: &ParsedProfile) -> Result<Transfer, IccProfileError> {
    let trcs = rgb_trcs(profile)?;
    let matrix_value = profile.rgb_to_xyz_matrix();
    if matrix_value.determinant().is_none() {
        return Err(IccProfileError::MalformedTransfer);
    }
    Ok(Transfer::new([
        matrix(&matrix_value.inverse(), [0.0; 3])?,
        TransferStage::curves(
            trcs.iter()
                .map(|value| inverse_curve(value))
                .collect::<Result<Vec<_>, _>>()?,
        )?,
    ])?)
}

fn gray_trc_to_pcs(profile: &ParsedProfile, pcs: Pcs) -> Result<Transfer, IccProfileError> {
    let trc = profile
        .gray_trc
        .as_ref()
        .ok_or(IccProfileError::MissingTransfer)?;
    let coefficients = match pcs {
        Pcs::XyzD50 => ICC_PCS_D50_XYZ,
        Pcs::LabD50 => [100.0, 0.0, 0.0],
    };
    Ok(Transfer::new([
        TransferStage::curves([curve(trc)?])?,
        TransferStage::Matrix(TransferMatrix::new(1, 3, coefficients, [0.0; 3])?),
    ])?)
}

fn gray_trc_from_pcs(profile: &ParsedProfile, pcs: Pcs) -> Result<Transfer, IccProfileError> {
    let trc = profile
        .gray_trc
        .as_ref()
        .ok_or(IccProfileError::MissingTransfer)?;
    let coefficients = match pcs {
        Pcs::XyzD50 => [0.0, 1.0, 0.0],
        Pcs::LabD50 => [0.01, 0.0, 0.0],
    };
    Ok(Transfer::new([
        TransferStage::Matrix(TransferMatrix::new(3, 1, coefficients, [0.0])?),
        TransferStage::curves([inverse_curve(trc)?])?,
    ])?)
}

#[derive(Debug)]
pub enum IccProfileError {
    Parse(moxcms::CmsError),
    UnsupportedVersion,
    UnsupportedProfileClass,
    UnsupportedPcs,
    MissingMediaWhitePoint,
    MissingTransfer,
    MalformedTransfer,
    Transfer(TransferError),
}

impl From<TransferError> for IccProfileError {
    fn from(error: TransferError) -> Self {
        Self::Transfer(error)
    }
}

impl fmt::Display for IccProfileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(error) => error.fmt(f),
            Self::UnsupportedVersion => f.write_str("only ICC v2 and v4 profiles are supported"),
            Self::UnsupportedProfileClass => {
                f.write_str("the ICC profile class does not connect device coordinates to a PCS")
            }
            Self::UnsupportedPcs => f.write_str("the ICC profile uses an unsupported PCS"),
            Self::MissingMediaWhitePoint => {
                f.write_str("absolute-colorimetric ICC links require a media white point")
            }
            Self::MissingTransfer => {
                f.write_str("the ICC profile has no transfer for this direction and intent")
            }
            Self::MalformedTransfer => {
                f.write_str("the ICC profile transfer is malformed or unsupported")
            }
            Self::Transfer(error) => error.fmt(f),
        }
    }
}

impl Error for IccProfileError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Parse(error) => Some(error),
            Self::Transfer(error) => Some(error),
            _ => None,
        }
    }
}

impl ColorProfile for IccColorProfile {
    fn model(&self) -> ColorModel {
        self.data.device_model()
    }

    fn connection_space(&self) -> ConnectionSpace {
        self.data.pcs.into()
    }

    fn transfer_to_connection_space(&self) -> Result<Transfer, SpaceConnectionError> {
        Ok(self.data.space_to_pcs(self.intent)?)
    }

    fn transfer_from_connection_space(&self) -> Result<Transfer, SpaceConnectionError> {
        Ok(self.data.pcs_to_space(self.intent)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parametric_trc_is_lowered_without_icc_function_numbers() {
        assert_eq!(parametric_curve(&[2.0]).unwrap().evaluate(0.5), 0.25);
        let type_one = parametric_curve(&[2.0, 2.0, -0.5]).unwrap();
        assert_eq!(type_one.evaluate(0.2), 0.0);
        assert_eq!(type_one.evaluate(0.5), 0.25);
        let type_two = parametric_curve(&[2.0, 2.0, -0.5, 0.1]).unwrap();
        assert_eq!(type_two.evaluate(0.2), 0.1);
        assert_eq!(type_two.evaluate(0.5), 0.35);
        let type_three = parametric_curve(&[2.0, 1.0, 0.0, 0.5, 0.3]).unwrap();
        assert_eq!(type_three.evaluate(0.2), 0.1);
        assert_eq!(type_three.evaluate(0.3), 0.09);

        let curve = parametric_curve(&[
            2.4,
            1.0 / 1.055,
            0.055 / 1.055,
            1.0 / 12.92,
            0.04045,
            0.0,
            0.0,
        ])
        .unwrap();
        for input in [0.0, 0.02, 0.04, 0.05, 0.25, 0.5, 1.0] {
            assert!(
                (curve.evaluate(input) - TransferCurve::SRGB_DECODE.evaluate(input)).abs() < 1e-12
            );
        }
        assert_eq!(curve.evaluate(-0.5), 0.0);
        assert_eq!(curve.evaluate(1.5), 1.0);
        assert!(parametric_curve(&[1.0; 6]).is_err());

        // Type 4 may be discontinuous: the breakpoint belongs to the high branch.
        let discontinuous = parametric_curve(&[1.0, 1.0, 0.0, 0.0, 0.5, 0.0, 0.25]).unwrap();
        assert_eq!(discontinuous.evaluate(0.5), 0.5);
        assert_eq!(discontinuous.evaluate(0.5 - f64::EPSILON), 0.25);
    }

    #[test]
    fn icc_data_model_is_not_inferred_from_channel_count() {
        assert_eq!(color_model(DataColorSpace::Rgb), ColorModel::Rgb);
        assert_eq!(color_model(DataColorSpace::Xyz), ColorModel::Xyz);
        assert_eq!(color_model(DataColorSpace::Color3), ColorModel::Channels(3));
        assert_eq!(color_model(DataColorSpace::Cmyk).channels(), 4);
    }

    #[test]
    fn real_v4_profiles_round_trip_device_coordinates() {
        for (bytes, sample) in [
            (
                &include_bytes!("color/fixtures/sRGB-v4.icc")[..],
                &[0.25, 0.5, 0.75][..],
            ),
            (
                &include_bytes!("color/fixtures/DisplayP3-v4.icc")[..],
                &[0.25, 0.5, 0.75][..],
            ),
            (
                &include_bytes!("color/fixtures/sGrey-v4.icc")[..],
                &[0.5][..],
            ),
        ] {
            let profile = IccProfile::from_bytes(bytes).unwrap();
            assert_eq!(profile.device_channels(), sample.len());
            let forward = profile
                .inner
                .space_to_pcs(IccRenderingIntent::RelativeColorimetric)
                .unwrap();
            let backward = profile
                .inner
                .pcs_to_space(IccRenderingIntent::RelativeColorimetric)
                .unwrap();
            let pcs = forward.evaluate(sample).unwrap();
            let restored = backward.evaluate(&pcs).unwrap();
            for (&actual, &expected) in restored.iter().zip(sample) {
                assert!((actual - expected).abs() < 0.01, "{actual} != {expected}");
            }
        }
    }

    #[test]
    fn loaded_profile_retains_exact_bytes_for_image_metadata() {
        let bytes = include_bytes!("color/fixtures/DisplayP3-v4.icc");
        let profile = IccProfile::from_bytes(bytes).unwrap();
        assert_eq!(profile.original_bytes(), Some(bytes.as_slice()));
        assert_eq!(profile.clone().original_bytes(), Some(bytes.as_slice()));
    }

    fn simple_lut(curve: ToneReprCurve) -> LutWarehouse {
        LutWarehouse::Multidimensional(LutMultidimensionalType {
            num_input_channels: 3,
            num_output_channels: 3,
            grid_points: [0; 16],
            clut: None,
            a_curves: Vec::new(),
            b_curves: vec![curve; 3],
            m_curves: Vec::new(),
            matrix: moxcms::Matrix3d::IDENTITY,
            bias: moxcms::Vector3d { v: [0.0; 3] },
        })
    }

    #[test]
    fn lut_selection_falls_back_by_direction_before_matrix_trc() {
        let matrix_only = IccProfile::from_parsed(ParsedProfile::new_srgb(), Pcs::XyzD50);
        let mut raw = ParsedProfile::new_srgb();
        raw.lut_a_to_b_perceptual = Some(simple_lut(ToneReprCurve::Lut(Vec::new())));
        raw.lut_a_to_b_colorimetric = Some(simple_lut(ToneReprCurve::Lut(vec![512])));
        let profile = IccProfile::from_parsed(raw, Pcs::XyzD50);

        let perceptual = profile
            .inner
            .space_to_pcs(IccRenderingIntent::Perceptual)
            .unwrap();
        let relative = profile
            .inner
            .space_to_pcs(IccRenderingIntent::RelativeColorimetric)
            .unwrap();
        let saturation = profile
            .inner
            .space_to_pcs(IccRenderingIntent::Saturation)
            .unwrap();
        assert_eq!(saturation, perceptual);
        let sample = [0.5; 3];
        assert!(relative.evaluate(&sample).unwrap()[0] < perceptual.evaluate(&sample).unwrap()[0]);

        // AToB LUTs do not prevent the opposite direction from using the
        // matrix/TRC model when no BToA LUT is present.
        let reverse = profile
            .inner
            .pcs_to_space(IccRenderingIntent::RelativeColorimetric)
            .unwrap();
        let matrix_reverse = matrix_only
            .inner
            .pcs_to_space(IccRenderingIntent::RelativeColorimetric)
            .unwrap();
        assert_eq!(reverse, matrix_reverse);
        assert_eq!(
            profile
                .inner
                .pcs_to_space(IccRenderingIntent::Saturation)
                .unwrap(),
            matrix_reverse
        );
        assert_eq!(
            matrix_only
                .inner
                .space_to_pcs(IccRenderingIntent::Perceptual)
                .unwrap(),
            matrix_only
                .inner
                .space_to_pcs(IccRenderingIntent::RelativeColorimetric)
                .unwrap()
        );
    }

    #[test]
    fn selected_color_space_uses_its_own_profile_intent() {
        let mut raw = ParsedProfile::new_srgb();
        raw.lut_a_to_b_perceptual = Some(simple_lut(ToneReprCurve::Lut(Vec::new())));
        raw.lut_a_to_b_colorimetric = Some(simple_lut(ToneReprCurve::Lut(vec![512])));
        let profile = IccProfile::from_parsed(raw, Pcs::XyzD50);
        let perceptual = profile.color_space(IccRenderingIntent::Perceptual);
        let relative = profile.color_space(IccRenderingIntent::RelativeColorimetric);

        assert_ne!(perceptual, relative);
        assert_ne!(
            perceptual.transfer_to_connection_space().unwrap(),
            relative.transfer_to_connection_space().unwrap()
        );
    }

    #[test]
    fn pcs_to_device_uses_its_own_zero_lut_before_matrix_trc() {
        let mut raw = ParsedProfile::new_srgb();
        raw.lut_b_to_a_perceptual = Some(simple_lut(ToneReprCurve::Lut(Vec::new())));
        let profile = IccProfile::from_parsed(raw, Pcs::XyzD50);
        let perceptual = profile
            .inner
            .pcs_to_space(IccRenderingIntent::Perceptual)
            .unwrap();
        let saturation = profile
            .inner
            .pcs_to_space(IccRenderingIntent::Saturation)
            .unwrap();
        assert_eq!(saturation, perceptual);
        assert_ne!(
            saturation,
            IccProfile::from_parsed(ParsedProfile::new_srgb(), Pcs::XyzD50)
                .inner
                .pcs_to_space(IccRenderingIntent::Saturation)
                .unwrap()
        );
    }

    #[test]
    fn parses_matrix_trc_profile_and_connects_both_directions() {
        let bytes = ParsedProfile::new_srgb().encode().unwrap();
        let profile = IccProfile::from_bytes(&bytes).unwrap();
        assert_eq!(profile.inner.pcs, Pcs::XyzD50);
        let forward = profile
            .inner
            .space_to_pcs(IccRenderingIntent::RelativeColorimetric)
            .unwrap();
        let backward = profile
            .inner
            .pcs_to_space(IccRenderingIntent::RelativeColorimetric)
            .unwrap();
        let xyz = forward.evaluate(&[0.4, 0.5, 0.6]).unwrap();
        let rgb = backward.evaluate(&xyz).unwrap();
        for (actual, expected) in rgb.into_iter().zip([0.4, 0.5, 0.6]) {
            assert!((actual - expected).abs() < 0.003);
        }
        assert!(forward.then(&backward).is_ok());
        let semantic = profile
            .color_space(IccRenderingIntent::RelativeColorimetric)
            .transfer_to(profile.color_space(IccRenderingIntent::RelativeColorimetric))
            .unwrap();
        let restored = semantic.evaluate(&[0.4, 0.5, 0.6]).unwrap();
        for (actual, expected) in restored.into_iter().zip([0.4, 0.5, 0.6]) {
            assert!((actual - expected).abs() < 0.003);
        }
        assert!(
            profile
                .inner
                .space_to_pcs(IccRenderingIntent::AbsoluteColorimetric)
                .is_ok()
        );
    }

    #[test]
    fn icc_profiles_with_different_pcs_connect_through_a_semantic_bridge() {
        let source = IccProfile::from_parsed(ParsedProfile::new_gray_with_gamma(1.0), Pcs::XyzD50);
        let destination =
            IccProfile::from_parsed(ParsedProfile::new_gray_with_gamma(1.0), Pcs::LabD50);
        let connected = source
            .color_space(IccRenderingIntent::RelativeColorimetric)
            .transfer_to(destination.color_space(IccRenderingIntent::RelativeColorimetric))
            .unwrap();
        let result = connected.evaluate(&[0.4]).unwrap();
        let expected = (116.0 * 0.4_f64.cbrt() - 16.0) / 100.0;
        assert!((result[0] - expected).abs() < 1e-8);
    }

    #[test]
    fn single_entry_gamma_trcs_invert_for_rgb_too() {
        let mut raw = ParsedProfile::new_srgb();
        for trc in [&mut raw.red_trc, &mut raw.green_trc, &mut raw.blue_trc] {
            *trc = Some(ToneReprCurve::Lut(vec![512]));
        }
        let profile = IccProfile::from_parsed(raw, Pcs::XyzD50);
        let forward = profile
            .inner
            .space_to_pcs(IccRenderingIntent::RelativeColorimetric)
            .unwrap();
        let backward = profile
            .inner
            .pcs_to_space(IccRenderingIntent::RelativeColorimetric)
            .unwrap();
        let sample = [0.3, 0.5, 0.7];
        let xyz = forward.evaluate(&sample).unwrap();
        let rgb = backward.evaluate(&xyz).unwrap();
        for (actual, expected) in rgb.into_iter().zip(sample) {
            assert!((actual - expected).abs() < 1e-10);
        }
    }

    #[test]
    fn parses_gray_trc_profile_and_connects_both_directions() {
        let bytes = ParsedProfile::new_gray_with_gamma(2.2).encode().unwrap();
        let profile = IccProfile::from_bytes(&bytes).unwrap();
        assert_eq!(profile.device_channels(), 1);
        let forward = profile
            .inner
            .space_to_pcs(IccRenderingIntent::RelativeColorimetric)
            .unwrap();
        let backward = profile
            .inner
            .pcs_to_space(IccRenderingIntent::RelativeColorimetric)
            .unwrap();
        let xyz = forward.evaluate(&[0.5]).unwrap();
        let white = ICC_PCS_D50_XYZ;
        for (actual, expected) in xyz.iter().zip(white) {
            assert!((actual - expected * 0.5_f64.powf(2.2)).abs() < 0.002);
        }
        let gray = backward.evaluate(&xyz).unwrap();
        assert!((gray[0] - 0.5).abs() < 0.002, "gray={gray:?}, xyz={xyz:?}");
        assert!(forward.then(&backward).is_ok());
    }

    #[test]
    fn absolute_intent_scales_relative_xyz_by_media_white() {
        let mut raw = ParsedProfile::new_srgb();
        raw.media_white_point = Some(moxcms::Xyzd {
            x: 0.75,
            y: 0.8,
            z: 0.6,
        });
        let profile = IccProfile::from_parsed(raw, Pcs::XyzD50);
        let relative = profile
            .inner
            .space_to_pcs(IccRenderingIntent::RelativeColorimetric)
            .unwrap();
        let absolute = profile
            .inner
            .space_to_pcs(IccRenderingIntent::AbsoluteColorimetric)
            .unwrap();
        let sample = [0.3, 0.5, 0.7];
        let relative_xyz = relative.evaluate(&sample).unwrap();
        let absolute_xyz = absolute.evaluate(&sample).unwrap();
        for ((actual, relative), (media, pcs)) in absolute_xyz
            .iter()
            .zip(&relative_xyz)
            .zip([0.75, 0.8, 0.6].into_iter().zip(ICC_PCS_D50_XYZ))
        {
            assert!((actual - relative * media / pcs).abs() < 1e-12);
        }
        let restored = profile
            .inner
            .pcs_to_space(IccRenderingIntent::AbsoluteColorimetric)
            .unwrap()
            .evaluate(&absolute_xyz)
            .unwrap();
        for (actual, expected) in restored.into_iter().zip(sample) {
            assert!((actual - expected).abs() < 0.003);
        }
        let connected = profile
            .color_space(IccRenderingIntent::AbsoluteColorimetric)
            .transfer_to(profile.color_space(IccRenderingIntent::RelativeColorimetric))
            .unwrap();
        let expected = profile
            .inner
            .pcs_to_space(IccRenderingIntent::RelativeColorimetric)
            .unwrap()
            .evaluate(&absolute_xyz)
            .unwrap();
        let actual = connected.evaluate(&sample).unwrap();
        for (actual, expected) in actual.into_iter().zip(expected) {
            assert!((actual - expected).abs() < 1e-12);
        }
    }

    #[test]
    fn absolute_lab_intent_adjusts_in_xyz() {
        let mut raw = ParsedProfile::new_gray_with_gamma(1.0);
        raw.media_white_point = Some(moxcms::Xyzd {
            x: 0.75,
            y: 0.8,
            z: 0.6,
        });
        let profile = IccProfile::from_parsed(raw, Pcs::LabD50);
        let absolute_lab = profile
            .inner
            .space_to_pcs(IccRenderingIntent::AbsoluteColorimetric)
            .unwrap()
            .evaluate(&[1.0])
            .unwrap();
        let xyz = LabD50::to_xyz_transfer().evaluate(&absolute_lab).unwrap();
        for (actual, expected) in xyz.into_iter().zip([0.75, 0.8, 0.6]) {
            assert!((actual - expected).abs() < 1e-8);
        }
        let restored = profile
            .inner
            .pcs_to_space(IccRenderingIntent::AbsoluteColorimetric)
            .unwrap()
            .evaluate(&absolute_lab)
            .unwrap();
        assert!((restored[0] - 1.0).abs() < 0.001);
    }

    #[test]
    fn absolute_intent_requires_valid_media_white() {
        let mut raw = ParsedProfile::new_srgb();
        raw.media_white_point = None;
        let profile = IccProfile::from_parsed(raw, Pcs::XyzD50);
        assert!(matches!(
            profile
                .inner
                .space_to_pcs(IccRenderingIntent::AbsoluteColorimetric),
            Err(IccProfileError::MissingMediaWhitePoint)
        ));
        let mut raw = ParsedProfile::new_srgb();
        raw.media_white_point = Some(moxcms::Xyzd {
            x: 0.0,
            y: 1.0,
            z: 1.0,
        });
        let profile = IccProfile::from_parsed(raw, Pcs::XyzD50);
        assert!(matches!(
            profile
                .inner
                .space_to_pcs(IccRenderingIntent::AbsoluteColorimetric),
            Err(IccProfileError::MalformedTransfer)
        ));
    }

    #[test]
    fn direct_absolute_mpe_does_not_require_media_white() {
        let mut raw = ParsedProfile::new_srgb();
        raw.media_white_point = None;
        let mut profile = IccProfile::from_parsed(raw, Pcs::XyzD50);
        Arc::get_mut(&mut profile.inner).unwrap().mpe.to_pcs[3] = Some(Transfer::IDENTITY);
        let actual = profile
            .inner
            .space_to_pcs(IccRenderingIntent::AbsoluteColorimetric)
            .unwrap()
            .evaluate(&[0.2, 0.4, 0.6])
            .unwrap();
        assert_eq!(actual, vec![0.2, 0.4, 0.6]);
    }

    #[test]
    fn gray_trc_fallback_does_not_accept_wrong_channel_count_from_lut() {
        let mut raw = ParsedProfile::new_gray_with_gamma(2.2);
        raw.lut_a_to_b_perceptual = Some(simple_lut(ToneReprCurve::Lut(Vec::new())));
        let profile = IccProfile::from_parsed(raw, Pcs::XyzD50);
        assert!(
            profile
                .inner
                .pcs_to_space(IccRenderingIntent::RelativeColorimetric)
                .is_ok()
        );
        let result = profile.inner.space_to_pcs(IccRenderingIntent::Perceptual);
        assert!(matches!(result, Err(IccProfileError::MalformedTransfer)));
    }

    #[test]
    fn loaded_spaces_are_distinct_while_their_transfers_can_connect() {
        let bytes = ParsedProfile::new_srgb().encode().unwrap();
        let source = IccProfile::from_bytes(&bytes).unwrap();
        let same_source = source.clone();
        let separate_load = IccProfile::from_bytes(&bytes).unwrap();
        assert_eq!(
            source.color_space(IccRenderingIntent::RelativeColorimetric),
            same_source.color_space(IccRenderingIntent::RelativeColorimetric)
        );
        assert_ne!(
            source.color_space(IccRenderingIntent::RelativeColorimetric),
            separate_load.color_space(IccRenderingIntent::RelativeColorimetric)
        );

        let to_pcs = source
            .inner
            .space_to_pcs(IccRenderingIntent::RelativeColorimetric)
            .unwrap();
        let from_pcs = separate_load
            .inner
            .pcs_to_space(IccRenderingIntent::RelativeColorimetric)
            .unwrap();
        let connected = to_pcs.then(&from_pcs).unwrap();
        assert_eq!(connected.input_channels(), Some(3));
        assert_eq!(connected.output_channels(), Some(3));
    }

    #[test]
    fn loaded_profile_and_selected_intent_define_runtime_space_identity() {
        let bytes = ParsedProfile::new_srgb().encode().unwrap();
        let first = IccProfile::from_bytes(&bytes).unwrap();
        let second = IccProfile::from_bytes(&bytes).unwrap();

        assert_eq!(
            first.color_space(IccRenderingIntent::RelativeColorimetric),
            first
                .clone()
                .color_space(IccRenderingIntent::RelativeColorimetric)
        );
        assert_ne!(
            first.color_space(IccRenderingIntent::RelativeColorimetric),
            second.color_space(IccRenderingIntent::RelativeColorimetric)
        );
        assert_ne!(
            first.color_space(IccRenderingIntent::RelativeColorimetric),
            first.color_space(IccRenderingIntent::Perceptual)
        );
        let spaces = std::collections::HashSet::from([
            first.color_space(IccRenderingIntent::RelativeColorimetric),
            first.color_space(IccRenderingIntent::Perceptual),
            first.clone().color_space(IccRenderingIntent::Perceptual),
        ]);
        assert_eq!(spaces.len(), 2);
    }

    #[test]
    fn transfer_does_not_retain_parsed_profile() {
        let bytes = ParsedProfile::new_srgb().encode().unwrap();
        let profile = IccProfile::from_bytes(&bytes).unwrap();
        let parsed = Arc::downgrade(&profile.inner.parsed);
        let transform = profile
            .inner
            .space_to_pcs(IccRenderingIntent::RelativeColorimetric)
            .unwrap();
        drop(profile);
        assert!(parsed.upgrade().is_none());
        assert_eq!(transform.input_channels(), Some(3));
    }

    #[test]
    fn erased_profile_handle_shares_the_icc_allocation() {
        let bytes = ParsedProfile::new_srgb().encode().unwrap();
        let profile = IccProfile::from_bytes(&bytes).unwrap();
        let parsed = Arc::downgrade(&profile.inner.parsed);
        let erased = profile.color_space(IccRenderingIntent::RelativeColorimetric);
        assert_eq!(
            erased,
            profile.color_space(IccRenderingIntent::RelativeColorimetric)
        );

        drop(profile);
        assert!(parsed.upgrade().is_some());
        drop(erased);
        assert!(parsed.upgrade().is_none());
    }

    #[test]
    fn accepts_v2_matrix_profile() {
        let mut bytes = ParsedProfile::new_srgb().encode().unwrap();
        bytes[8] = 2;
        bytes[9] = 0x10;
        let profile = IccProfile::from_bytes(&bytes).unwrap();
        let transfer = profile
            .inner
            .space_to_pcs(IccRenderingIntent::RelativeColorimetric)
            .unwrap();
        let xyz = transfer.evaluate(&[1.0, 1.0, 1.0]).unwrap();
        assert!((xyz[1] - 1.0).abs() < 0.01);
    }

    #[test]
    fn rejects_profile_classes_without_device_to_pcs_semantics() {
        let mut bytes = ParsedProfile::new_srgb().encode().unwrap();
        bytes[12..16].copy_from_slice(b"link");
        assert!(matches!(
            IccProfile::from_bytes(&bytes),
            Err(IccProfileError::UnsupportedProfileClass)
        ));
    }

    #[test]
    fn parsed_lut_uses_actual_xyz_pcs_values_not_encoded_values() {
        let mut raw = ParsedProfile::new_srgb();
        let identity = ToneReprCurve::Lut(Vec::new());
        let lut = LutMultidimensionalType {
            num_input_channels: 3,
            num_output_channels: 3,
            grid_points: [0; 16],
            clut: None,
            a_curves: Vec::new(),
            b_curves: vec![identity.clone(), identity.clone(), identity],
            m_curves: Vec::new(),
            matrix: moxcms::Matrix3d::IDENTITY,
            bias: moxcms::Vector3d { v: [0.0; 3] },
        };
        raw.lut_a_to_b_colorimetric = Some(LutWarehouse::Multidimensional(lut.clone()));
        raw.lut_b_to_a_colorimetric = Some(LutWarehouse::Multidimensional(lut));
        let profile = IccProfile::from_bytes(&raw.encode().unwrap()).unwrap();
        let forward = profile
            .inner
            .space_to_pcs(IccRenderingIntent::RelativeColorimetric)
            .unwrap();
        let backward = profile
            .inner
            .pcs_to_space(IccRenderingIntent::RelativeColorimetric)
            .unwrap();
        let encoded = [0.25, 0.5, 0.75];
        let xyz = forward.evaluate(&encoded).unwrap();
        assert!((xyz[1] - 65535.0 / 65536.0).abs() < 1e-12);
        let restored = backward.evaluate(&xyz).unwrap();
        for (actual, expected) in restored.into_iter().zip(encoded) {
            assert!((actual - expected).abs() < 1e-12);
        }
    }

    #[test]
    fn legacy_lab_encoding_differs_from_v4_lut_encoding() {
        let lut = LutDataType {
            num_input_channels: 3,
            num_output_channels: 3,
            num_clut_grid_points: 2,
            matrix: moxcms::Matrix3d::IDENTITY,
            num_input_table_entries: 2,
            num_output_table_entries: 2,
            input_table: LutStore::Store16([0, 65535].repeat(3)),
            clut_table: LutStore::Store16(vec![0; 24]),
            output_table: LutStore::Store16([0, 65535].repeat(3)),
            lut_type: LutType::Lut16,
        };
        let tag = LutWarehouse::Lut(lut);
        let decoded = Transfer::new([pcs_decoding(Pcs::LabD50, &tag).unwrap()]).unwrap();
        let lab = decoded
            .evaluate(&[65280.0 / 65535.0, 32768.0 / 65535.0, 32768.0 / 65535.0])
            .unwrap();
        assert!((lab[0] - 100.0).abs() < 1e-10);
        assert!(lab[1].abs() < 1e-10);
        assert!(lab[2].abs() < 1e-10);
    }

    #[test]
    fn four_channel_clut_evaluates_on_cpu() {
        let mut raw = ParsedProfile::new_srgb();
        raw.color_space = DataColorSpace::Cmyk;
        let mut grid_points = [0; 16];
        grid_points[..4].fill(2);
        let mut samples = Vec::new();
        for index in 0..16 {
            samples.extend([
                if index & 8 != 0 { 65535 } else { 0 },
                if index & 4 != 0 { 65535 } else { 0 },
                if index & 2 != 0 { 65535 } else { 0 },
            ]);
        }
        raw.lut_a_to_b_colorimetric =
            Some(LutWarehouse::Multidimensional(LutMultidimensionalType {
                num_input_channels: 4,
                num_output_channels: 3,
                grid_points,
                clut: Some(LutStore::Store16(samples)),
                a_curves: vec![ToneReprCurve::Lut(Vec::new()); 4],
                b_curves: vec![ToneReprCurve::Lut(Vec::new()); 3],
                m_curves: Vec::new(),
                matrix: moxcms::Matrix3d::IDENTITY,
                bias: moxcms::Vector3d { v: [0.0; 3] },
            }));
        let profile = IccProfile::from_parsed(raw, Pcs::XyzD50);
        assert_eq!(profile.device_channels(), 4);
        assert!(matches!(
            profile
                .inner
                .pcs_to_space(IccRenderingIntent::RelativeColorimetric),
            Err(IccProfileError::MissingTransfer)
        ));
        let transfer = profile
            .inner
            .space_to_pcs(IccRenderingIntent::RelativeColorimetric)
            .unwrap();
        let result = transfer.evaluate(&[0.8, 0.2, 0.6, 0.5]).unwrap();
        let xyz_scale = 65535.0 / 32768.0;
        for (actual, expected) in result.into_iter().zip([0.8, 0.2, 0.6]) {
            assert!((actual - expected * xyz_scale).abs() < 1e-8);
        }
    }
}
