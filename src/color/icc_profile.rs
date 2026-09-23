use std::{error::Error, fmt};

use moxcms::{
    ColorProfile as ParsedProfile, DataColorSpace, LutDataType, LutMultidimensionalType, LutStore,
    LutType, LutWarehouse, ProfileVersion, ToneReprCurve,
};

use super::{
    IccPcs, IccTransfer, IccTransferDirection, IccTransferError, Transfer, TransferClut,
    TransferCurve, TransferError, TransferMatrix, TransferStage,
};

/// The rendering-intent tag selected from a profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IccRenderingIntent {
    Perceptual,
    RelativeColorimetric,
    Saturation,
    AbsoluteColorimetric,
}

/// A parsed ICC v2/v4 profile whose supported transforms can be lowered to
/// [`Transfer`] for both CPU evaluation and WGSL generation.
///
/// A profile is not itself a single transform: the direction and rendering
/// intent select an AToB/BToA tag. PCS values exposed by its transfers are
/// actual D50 XYZ or Lab coordinates, not the normalized integer encodings
/// used inside LUT tags.
#[derive(Clone, Debug)]
pub struct IccProfile {
    parsed: ParsedProfile,
    pcs: IccPcs,
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
        let pcs = match parsed.pcs {
            DataColorSpace::Xyz => IccPcs::XyzD50,
            DataColorSpace::Lab => IccPcs::LabD50,
            _ => return Err(IccProfileError::UnsupportedPcs),
        };
        Ok(Self { parsed, pcs })
    }

    pub const fn pcs(&self) -> IccPcs {
        self.pcs
    }

    pub fn device_channels(&self) -> usize {
        color_space_channels(self.parsed.color_space)
    }

    pub fn transfer(
        &self,
        direction: IccTransferDirection,
        intent: IccRenderingIntent,
    ) -> Result<IccTransfer, IccProfileError> {
        // Absolute colorimetry requires a media-white adjustment in addition
        // to the relative-colorimetric tag; that link is not represented yet.
        if intent == IccRenderingIntent::AbsoluteColorimetric {
            return Err(IccProfileError::UnsupportedIntent);
        }
        let tag = match (direction, intent) {
            (IccTransferDirection::DeviceToPcs, IccRenderingIntent::Perceptual) => {
                &self.parsed.lut_a_to_b_perceptual
            }
            (IccTransferDirection::DeviceToPcs, IccRenderingIntent::RelativeColorimetric) => {
                &self.parsed.lut_a_to_b_colorimetric
            }
            (IccTransferDirection::DeviceToPcs, IccRenderingIntent::Saturation) => {
                &self.parsed.lut_a_to_b_saturation
            }
            (IccTransferDirection::PcsToDevice, IccRenderingIntent::Perceptual) => {
                &self.parsed.lut_b_to_a_perceptual
            }
            (IccTransferDirection::PcsToDevice, IccRenderingIntent::RelativeColorimetric) => {
                &self.parsed.lut_b_to_a_colorimetric
            }
            (IccTransferDirection::PcsToDevice, IccRenderingIntent::Saturation) => {
                &self.parsed.lut_b_to_a_saturation
            }
            (_, IccRenderingIntent::AbsoluteColorimetric) => unreachable!(),
        };
        let transfer = if let Some(tag) = tag {
            let mut stages = Vec::new();
            if direction == IccTransferDirection::PcsToDevice {
                stages.push(pcs_encoding(self.pcs, tag)?);
            }
            stages.extend(match tag {
                LutWarehouse::Lut(lut) => legacy_lut(lut, direction, self.pcs)?,
                LutWarehouse::Multidimensional(lut) => multidimensional_lut(lut, direction)?,
            });
            if direction == IccTransferDirection::DeviceToPcs {
                stages.push(pcs_decoding(self.pcs, tag)?);
            }
            let input_channels = if direction == IccTransferDirection::DeviceToPcs {
                self.device_channels()
            } else {
                3
            };
            Transfer::new(input_channels, stages)?
        } else if intent == IccRenderingIntent::RelativeColorimetric
            && !self.has_any_lut()
            && self.parsed.color_space == DataColorSpace::Rgb
            && self.pcs == IccPcs::XyzD50
        {
            rgb_matrix_trc(&self.parsed, direction)?
        } else {
            return Err(IccProfileError::MissingTransfer);
        };
        if direction == IccTransferDirection::DeviceToPcs {
            IccTransfer::device_to_pcs(self.pcs, transfer).map_err(Into::into)
        } else {
            IccTransfer::pcs_to_device(self.pcs, transfer).map_err(Into::into)
        }
    }

    fn has_any_lut(&self) -> bool {
        self.parsed.lut_a_to_b_perceptual.is_some()
            || self.parsed.lut_a_to_b_colorimetric.is_some()
            || self.parsed.lut_a_to_b_saturation.is_some()
            || self.parsed.lut_b_to_a_perceptual.is_some()
            || self.parsed.lut_b_to_a_colorimetric.is_some()
            || self.parsed.lut_b_to_a_saturation.is_some()
    }
}

fn color_space_channels(space: DataColorSpace) -> usize {
    match space {
        DataColorSpace::Gray => 1,
        DataColorSpace::Color2 => 2,
        DataColorSpace::Cmyk | DataColorSpace::Color4 => 4,
        DataColorSpace::Color5 => 5,
        DataColorSpace::Color6 => 6,
        DataColorSpace::Color7 => 7,
        DataColorSpace::Color8 => 8,
        DataColorSpace::Color9 => 9,
        DataColorSpace::Color10 => 10,
        DataColorSpace::Color11 => 11,
        DataColorSpace::Color12 => 12,
        DataColorSpace::Color13 => 13,
        DataColorSpace::Color14 => 14,
        DataColorSpace::Color15 => 15,
        _ => 3,
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
        ToneReprCurve::Parametric(values) => TransferCurve::icc_parametric(
            match values.len() {
                1 => 0,
                3 => 1,
                4 => 2,
                5 => 3,
                7 => 4,
                _ => return Err(IccProfileError::MalformedTransfer),
            },
            &values
                .iter()
                .map(|value| f64::from(*value))
                .collect::<Vec<_>>(),
        )?,
    })
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

fn legacy_lut(
    lut: &LutDataType,
    direction: IccTransferDirection,
    pcs: IccPcs,
) -> Result<Vec<TransferStage>, IccProfileError> {
    // ICC does not define an 8-bit PCSXYZ encoding for lut8Type.
    if lut.lut_type == LutType::Lut8 && pcs == IccPcs::XyzD50 {
        return Err(IccProfileError::MalformedTransfer);
    }
    let inputs = usize::from(lut.num_input_channels);
    let outputs = usize::from(lut.num_output_channels);
    let mut stages = Vec::new();
    // The legacy LUT matrix is only meaningful for a three-channel PCSXYZ
    // input; conforming tags use identity in all other cases.
    if direction == IccTransferDirection::PcsToDevice && pcs == IccPcs::XyzD50 && inputs == 3 {
        stages.push(matrix(&lut.matrix, [0.0; 3])?);
    }
    stages.push(table_curves(
        &lut.input_table,
        inputs,
        usize::from(lut.num_input_table_entries),
    )?);
    stages.push(clut(
        &vec![lut.num_clut_grid_points; inputs],
        outputs,
        &lut.clut_table,
    )?);
    stages.push(table_curves(
        &lut.output_table,
        outputs,
        usize::from(lut.num_output_table_entries),
    )?);
    Ok(stages)
}

fn multidimensional_lut(
    lut: &LutMultidimensionalType,
    direction: IccTransferDirection,
) -> Result<Vec<TransferStage>, IccProfileError> {
    let inputs = usize::from(lut.num_input_channels);
    let outputs = usize::from(lut.num_output_channels);
    if lut.b_curves.len() != 3
        || lut.a_curves.is_empty() != lut.clut.is_none()
        || !lut.m_curves.is_empty() && lut.m_curves.len() != 3
        || direction == IccTransferDirection::DeviceToPcs && outputs != 3
        || direction == IccTransferDirection::PcsToDevice && inputs != 3
    {
        return Err(IccProfileError::MalformedTransfer);
    }
    let mut stages = Vec::new();
    let matrix_bias = lut.bias.v;
    match direction {
        IccTransferDirection::DeviceToPcs => {
            if !lut.a_curves.is_empty() {
                stages.push(curves(&lut.a_curves, inputs)?);
            }
            if let Some(table) = &lut.clut {
                stages.push(clut(&lut.grid_points[..inputs], outputs, table)?);
            }
            if !lut.m_curves.is_empty() {
                stages.push(curves(&lut.m_curves, 3)?);
                stages.push(matrix(&lut.matrix, matrix_bias)?);
                stages.push(TransferStage::Clamp(3));
            }
            stages.push(curves(&lut.b_curves, 3)?);
        }
        IccTransferDirection::PcsToDevice => {
            stages.push(curves(&lut.b_curves, 3)?);
            if !lut.m_curves.is_empty() {
                stages.push(matrix(&lut.matrix, matrix_bias)?);
                stages.push(TransferStage::Clamp(3));
                stages.push(curves(&lut.m_curves, 3)?);
            }
            if let Some(table) = &lut.clut {
                stages.push(clut(&lut.grid_points[..3], outputs, table)?);
            }
            if !lut.a_curves.is_empty() {
                stages.push(curves(&lut.a_curves, outputs)?);
            }
        }
    }
    Ok(stages)
}

fn pcs_decoding(pcs: IccPcs, tag: &LutWarehouse) -> Result<TransferStage, IccProfileError> {
    let legacy_lab = matches!(tag, LutWarehouse::Lut(lut) if lut.lut_type == LutType::Lut16);
    let (scale, offsets) = match pcs {
        IccPcs::XyzD50 => ([65535.0 / 32768.0; 3], [0.0; 3]),
        IccPcs::LabD50 if legacy_lab => (
            [100.0 * 65535.0 / 65280.0, 65535.0 / 256.0, 65535.0 / 256.0],
            [0.0, -128.0, -128.0],
        ),
        IccPcs::LabD50 => ([100.0, 255.0, 255.0], [0.0, -128.0, -128.0]),
    };
    diagonal(scale, offsets)
}

fn pcs_encoding(pcs: IccPcs, tag: &LutWarehouse) -> Result<TransferStage, IccProfileError> {
    let legacy_lab = matches!(tag, LutWarehouse::Lut(lut) if lut.lut_type == LutType::Lut16);
    let (scale, offsets) = match pcs {
        IccPcs::XyzD50 => ([32768.0 / 65535.0; 3], [0.0; 3]),
        IccPcs::LabD50 if legacy_lab => (
            [
                65280.0 / (100.0 * 65535.0),
                256.0 / 65535.0,
                256.0 / 65535.0,
            ],
            [0.0, 128.0 * 256.0 / 65535.0, 128.0 * 256.0 / 65535.0],
        ),
        IccPcs::LabD50 => (
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

fn rgb_matrix_trc(
    profile: &ParsedProfile,
    direction: IccTransferDirection,
) -> Result<Transfer, IccProfileError> {
    let trcs = [
        profile.red_trc.as_ref(),
        profile.green_trc.as_ref(),
        profile.blue_trc.as_ref(),
    ];
    let trcs = trcs
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or(IccProfileError::MissingTransfer)?;
    let matrix_value = profile.rgb_to_xyz_matrix();
    let stages = match direction {
        IccTransferDirection::DeviceToPcs => vec![
            TransferStage::curves(
                trcs.iter()
                    .map(|value| curve(value))
                    .collect::<Result<Vec<_>, _>>()?,
            )?,
            matrix(&matrix_value, [0.0; 3])?,
        ],
        IccTransferDirection::PcsToDevice => {
            if matrix_value.determinant().is_none() {
                return Err(IccProfileError::MalformedTransfer);
            }
            let inverse = trcs
                .iter()
                .map(|value| value.inverse().map_err(IccProfileError::Parse))
                .collect::<Result<Vec<_>, _>>()?;
            vec![
                matrix(&matrix_value.inverse(), [0.0; 3])?,
                curves(&inverse, 3)?,
            ]
        }
    };
    Ok(Transfer::new(3, stages)?)
}

#[derive(Debug)]
pub enum IccProfileError {
    Parse(moxcms::CmsError),
    UnsupportedVersion,
    UnsupportedPcs,
    UnsupportedIntent,
    MissingTransfer,
    MalformedTransfer,
    Transfer(TransferError),
    IccTransfer(IccTransferError),
}

impl From<TransferError> for IccProfileError {
    fn from(error: TransferError) -> Self {
        Self::Transfer(error)
    }
}

impl From<IccTransferError> for IccProfileError {
    fn from(error: IccTransferError) -> Self {
        Self::IccTransfer(error)
    }
}

impl fmt::Display for IccProfileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(error) => error.fmt(f),
            Self::UnsupportedVersion => f.write_str("only ICC v2 and v4 profiles are supported"),
            Self::UnsupportedPcs => f.write_str("the ICC profile uses an unsupported PCS"),
            Self::UnsupportedIntent => {
                f.write_str("absolute-colorimetric ICC links are not supported")
            }
            Self::MissingTransfer => {
                f.write_str("the ICC profile has no transfer for this direction and intent")
            }
            Self::MalformedTransfer => {
                f.write_str("the ICC profile transfer is malformed or unsupported")
            }
            Self::Transfer(error) => error.fmt(f),
            Self::IccTransfer(error) => error.fmt(f),
        }
    }
}

impl Error for IccProfileError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Parse(error) => Some(error),
            Self::Transfer(error) => Some(error),
            Self::IccTransfer(error) => Some(error),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_matrix_trc_profile_and_connects_both_directions() {
        let bytes = ParsedProfile::new_srgb().encode().unwrap();
        let profile = IccProfile::from_bytes(&bytes).unwrap();
        assert_eq!(profile.pcs(), IccPcs::XyzD50);
        let forward = profile
            .transfer(
                IccTransferDirection::DeviceToPcs,
                IccRenderingIntent::RelativeColorimetric,
            )
            .unwrap();
        let backward = profile
            .transfer(
                IccTransferDirection::PcsToDevice,
                IccRenderingIntent::RelativeColorimetric,
            )
            .unwrap();
        let xyz = forward.transfer().evaluate(&[0.4, 0.5, 0.6]).unwrap();
        let rgb = backward.transfer().evaluate(&xyz).unwrap();
        for (actual, expected) in rgb.into_iter().zip([0.4, 0.5, 0.6]) {
            assert!((actual - expected).abs() < 0.003);
        }
        assert!(forward.connect(&backward).is_ok());
        assert!(matches!(
            profile.transfer(
                IccTransferDirection::DeviceToPcs,
                IccRenderingIntent::AbsoluteColorimetric
            ),
            Err(IccProfileError::UnsupportedIntent)
        ));
    }

    #[test]
    fn accepts_v2_matrix_profile() {
        let mut bytes = ParsedProfile::new_srgb().encode().unwrap();
        bytes[8] = 2;
        bytes[9] = 0x10;
        let profile = IccProfile::from_bytes(&bytes).unwrap();
        let transfer = profile
            .transfer(
                IccTransferDirection::DeviceToPcs,
                IccRenderingIntent::RelativeColorimetric,
            )
            .unwrap();
        let xyz = transfer.transfer().evaluate(&[1.0, 1.0, 1.0]).unwrap();
        assert!((xyz[1] - 1.0).abs() < 0.01);
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
            .transfer(
                IccTransferDirection::DeviceToPcs,
                IccRenderingIntent::RelativeColorimetric,
            )
            .unwrap();
        let backward = profile
            .transfer(
                IccTransferDirection::PcsToDevice,
                IccRenderingIntent::RelativeColorimetric,
            )
            .unwrap();
        let encoded = [0.25, 0.5, 0.75];
        let xyz = forward.transfer().evaluate(&encoded).unwrap();
        assert!((xyz[1] - 65535.0 / 65536.0).abs() < 1e-12);
        let restored = backward.transfer().evaluate(&xyz).unwrap();
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
        let decoded = Transfer::new(3, [pcs_decoding(IccPcs::LabD50, &tag).unwrap()]).unwrap();
        let lab = decoded
            .evaluate(&[65280.0 / 65535.0, 32768.0 / 65535.0, 32768.0 / 65535.0])
            .unwrap();
        assert!((lab[0] - 100.0).abs() < 1e-10);
        assert!(lab[1].abs() < 1e-10);
        assert!(lab[2].abs() < 1e-10);
    }

    #[test]
    fn four_channel_clut_lowers_to_cpu_and_shader() {
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
        let profile = IccProfile {
            parsed: raw,
            pcs: IccPcs::XyzD50,
        };
        assert_eq!(profile.device_channels(), 4);
        let transfer = profile
            .transfer(
                IccTransferDirection::DeviceToPcs,
                IccRenderingIntent::RelativeColorimetric,
            )
            .unwrap();
        let result = transfer.transfer().evaluate(&[0.8, 0.2, 0.6, 0.5]).unwrap();
        let xyz_scale = 65535.0 / 32768.0;
        for (actual, expected) in result.into_iter().zip([0.8, 0.2, 0.6]) {
            assert!((actual - expected * xyz_scale).abs() < 1e-8);
        }
        let shader = transfer.transfer().wgsl_with_storage(22, 1, 4).unwrap();
        assert_eq!(shader.table_values().len(), 48);
        let module = naga::front::wgsl::parse_str(shader.source())
            .unwrap_or_else(|error| panic!("{}", error.emit_to_string(shader.source())));
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap();
    }
}
