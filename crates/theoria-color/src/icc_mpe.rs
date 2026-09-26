use super::transfer::{
    MpeSegment, MpeSegmentKind, Transfer, TransferClut, TransferCurve, TransferMatrix,
    TransferStage,
};

#[derive(Clone, Debug, Default)]
pub(super) struct MpeTags {
    pub to_pcs: [Option<Transfer>; 4],
    pub from_pcs: [Option<Transfer>; 4],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct MpeParseError;

impl MpeTags {
    pub fn parse(profile: &[u8]) -> Result<Self, MpeParseError> {
        let count = usize::try_from(u32_at(profile, 128)?).map_err(|_| MpeParseError)?;
        let table_len = count.checked_mul(12).ok_or(MpeParseError)?;
        section(profile, 132, table_len)?;
        let mut tags = Self::default();
        for index in 0..count {
            let entry = 132 + index * 12;
            let signature = section(profile, entry, 4)?;
            let (direction, intent) = match signature {
                b"D2B0" => (true, 0),
                b"D2B1" => (true, 1),
                b"D2B2" => (true, 2),
                b"D2B3" => (true, 3),
                b"B2D0" => (false, 0),
                b"B2D1" => (false, 1),
                b"B2D2" => (false, 2),
                b"B2D3" => (false, 3),
                _ => continue,
            };
            let offset = usize::try_from(u32_at(profile, entry + 4)?).map_err(|_| MpeParseError)?;
            let size = usize::try_from(u32_at(profile, entry + 8)?).map_err(|_| MpeParseError)?;
            let data = section(profile, offset, size)?;
            let transfer = parse_mpe(data)?;
            if direction {
                tags.to_pcs[intent] = transfer;
            } else {
                tags.from_pcs[intent] = transfer;
            }
        }
        Ok(tags)
    }
}

fn parse_mpe(data: &[u8]) -> Result<Option<Transfer>, MpeParseError> {
    if section(data, 0, 4)? != b"mpet" {
        return Err(MpeParseError);
    }
    let inputs = usize::from(u16_at(data, 8)?);
    let outputs = usize::from(u16_at(data, 10)?);
    let count = usize::try_from(u32_at(data, 12)?).map_err(|_| MpeParseError)?;
    if inputs == 0 || outputs == 0 || count == 0 {
        return Err(MpeParseError);
    }
    section(data, 16, count.checked_mul(8).ok_or(MpeParseError)?)?;
    let mut stages = Vec::with_capacity(count);
    let mut channels = inputs;
    for index in 0..count {
        let entry = 16 + index * 8;
        let offset = usize::try_from(u32_at(data, entry)?).map_err(|_| MpeParseError)?;
        let size = usize::try_from(u32_at(data, entry + 4)?).map_err(|_| MpeParseError)?;
        let element = section(data, offset, size)?;
        let stage_inputs = usize::from(u16_at(element, 8)?);
        let stage_outputs = usize::from(u16_at(element, 10)?);
        if stage_inputs != channels || stage_outputs == 0 {
            return Err(MpeParseError);
        }
        let stage = match section(element, 0, 4)? {
            b"cvst" => Some(parse_curve_set(element, stage_inputs, stage_outputs)?),
            b"matf" => Some(parse_matrix(element, stage_inputs, stage_outputs)?),
            b"clut" => Some(parse_clut(element, stage_inputs, stage_outputs)?),
            b"bACS" | b"eACS" if stage_inputs == stage_outputs => {
                section(element, 12, 4)?;
                None
            }
            _ => return Ok(None),
        };
        if let Some(stage) = stage {
            stages.push(stage);
        }
        channels = stage_outputs;
    }
    if channels != outputs {
        return Err(MpeParseError);
    }
    Transfer::new(inputs, stages)
        .map(Some)
        .map_err(|_| MpeParseError)
}

fn parse_matrix(
    data: &[u8],
    inputs: usize,
    outputs: usize,
) -> Result<TransferStage, MpeParseError> {
    let coefficients = inputs.checked_mul(outputs).ok_or(MpeParseError)?;
    let total = coefficients.checked_add(outputs).ok_or(MpeParseError)?;
    section(data, 12, total.checked_mul(4).ok_or(MpeParseError)?)?;
    let values = (0..total)
        .map(|index| f32_at(data, 12 + 4 * index))
        .collect::<Result<Vec<_>, _>>()?;
    TransferMatrix::new(
        inputs,
        outputs,
        values[..coefficients].to_vec(),
        values[coefficients..].to_vec(),
    )
    .map(TransferStage::Matrix)
    .map_err(|_| MpeParseError)
}

fn parse_clut(data: &[u8], inputs: usize, outputs: usize) -> Result<TransferStage, MpeParseError> {
    if inputs > 16 {
        return Err(MpeParseError);
    }
    let grid = section(data, 12, 16)?[..inputs].to_vec();
    let entries = grid
        .iter()
        .try_fold(outputs, |count, points| {
            count.checked_mul(usize::from(*points))
        })
        .ok_or(MpeParseError)?;
    section(data, 28, entries.checked_mul(4).ok_or(MpeParseError)?)?;
    let values = (0..entries)
        .map(|index| f32_at(data, 28 + 4 * index))
        .collect::<Result<Vec<_>, _>>()?;
    TransferClut::new(grid, outputs, values)
        .map(TransferStage::Clut)
        .map_err(|_| MpeParseError)
}

fn parse_curve_set(
    data: &[u8],
    inputs: usize,
    outputs: usize,
) -> Result<TransferStage, MpeParseError> {
    if inputs != outputs {
        return Err(MpeParseError);
    }
    section(data, 12, inputs.checked_mul(8).ok_or(MpeParseError)?)?;
    let curves = (0..inputs)
        .map(|index| {
            let entry = 12 + index * 8;
            let offset = usize::try_from(u32_at(data, entry)?).map_err(|_| MpeParseError)?;
            let size = usize::try_from(u32_at(data, entry + 4)?).map_err(|_| MpeParseError)?;
            parse_curve(section(data, offset, size)?)
        })
        .collect::<Result<Vec<_>, _>>()?;
    TransferStage::curves(curves).map_err(|_| MpeParseError)
}

fn parse_curve(data: &[u8]) -> Result<TransferCurve, MpeParseError> {
    if section(data, 0, 4)? != b"curf" {
        return Err(MpeParseError);
    }
    let count = usize::from(u16_at(data, 8)?);
    if count == 0 {
        return Err(MpeParseError);
    }
    let breaks_len = (count - 1).checked_mul(4).ok_or(MpeParseError)?;
    section(data, 12, breaks_len)?;
    let breaks = (0..count - 1)
        .map(|index| f32_at(data, 12 + index * 4))
        .collect::<Result<Vec<_>, _>>()?;
    if breaks.windows(2).any(|pair| pair[0] > pair[1]) {
        return Err(MpeParseError);
    }
    let mut segments: Vec<MpeSegment> = Vec::with_capacity(count);
    let mut offset = 12 + breaks_len;
    for index in 0..count {
        let upper = breaks.get(index).copied().unwrap_or(f64::INFINITY);
        let signature = section(data, offset, 4)?;
        let (kind, length) = match signature {
            b"parf" => {
                let function = u16_at(data, offset + 8)?;
                let parameter_count = match function {
                    0 => 4,
                    1 | 2 => 5,
                    _ => return Err(MpeParseError),
                };
                let length = 12 + 4 * parameter_count;
                section(data, offset, length)?;
                let parameters = (0..parameter_count)
                    .map(|p| f32_at(data, offset + 12 + 4 * p))
                    .collect::<Result<Vec<_>, _>>()?;
                (
                    MpeSegmentKind::Formula {
                        function,
                        parameters,
                    },
                    length,
                )
            }
            b"samf" if index > 0 && index + 1 < count => {
                let sample_count =
                    usize::try_from(u32_at(data, offset + 8)?).map_err(|_| MpeParseError)?;
                if sample_count == 0 {
                    return Err(MpeParseError);
                }
                let length = 12usize
                    .checked_add(sample_count.checked_mul(4).ok_or(MpeParseError)?)
                    .ok_or(MpeParseError)?;
                section(data, offset, length)?;
                let lower = breaks[index - 1];
                let initial = segments.last().ok_or(MpeParseError)?.evaluate(lower);
                if !initial.is_finite() {
                    return Err(MpeParseError);
                }
                let mut samples = Vec::with_capacity(sample_count + 1);
                samples.push(initial);
                for sample in 0..sample_count {
                    samples.push(f32_at(data, offset + 12 + 4 * sample)?);
                }
                (MpeSegmentKind::Sampled { lower, samples }, length)
            }
            _ => return Err(MpeParseError),
        };
        segments.push(MpeSegment { upper, kind });
        offset = offset.checked_add(length).ok_or(MpeParseError)?;
    }
    TransferCurve::mpe(segments).map_err(|_| MpeParseError)
}

fn section(bytes: &[u8], offset: usize, length: usize) -> Result<&[u8], MpeParseError> {
    let end = offset.checked_add(length).ok_or(MpeParseError)?;
    bytes.get(offset..end).ok_or(MpeParseError)
}

fn u16_at(bytes: &[u8], offset: usize) -> Result<u16, MpeParseError> {
    Ok(u16::from_be_bytes(
        section(bytes, offset, 2)?.try_into().unwrap(),
    ))
}

fn u32_at(bytes: &[u8], offset: usize) -> Result<u32, MpeParseError> {
    Ok(u32::from_be_bytes(
        section(bytes, offset, 4)?.try_into().unwrap(),
    ))
}

fn f32_at(bytes: &[u8], offset: usize) -> Result<f64, MpeParseError> {
    let value = f32::from_bits(u32_at(bytes, offset)?) as f64;
    if value.is_finite() {
        Ok(value)
    } else {
        Err(MpeParseError)
    }
}

#[cfg(test)]
mod tests {
    use moxcms::ColorProfile as ParsedProfile;

    use super::*;
    use crate::{IccProfile, IccRenderingIntent};

    fn matrix_element(scale: f32) -> Vec<u8> {
        let mut element = Vec::new();
        element.extend_from_slice(b"matf");
        element.extend_from_slice(&[0; 4]);
        element.extend_from_slice(&3u16.to_be_bytes());
        element.extend_from_slice(&3u16.to_be_bytes());
        for row in 0..3 {
            for column in 0..3 {
                element.extend_from_slice(&(if row == column { scale } else { 0.0 }).to_be_bytes());
            }
        }
        for _ in 0..3 {
            element.extend_from_slice(&0f32.to_be_bytes());
        }
        element
    }

    fn mpe_tag(inputs: u16, outputs: u16, elements: &[Vec<u8>]) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(b"mpet");
        data.extend_from_slice(&[0; 4]);
        data.extend_from_slice(&inputs.to_be_bytes());
        data.extend_from_slice(&outputs.to_be_bytes());
        data.extend_from_slice(&(elements.len() as u32).to_be_bytes());
        let mut offset = (16 + 8 * elements.len()) as u32;
        for element in elements {
            data.extend_from_slice(&offset.to_be_bytes());
            data.extend_from_slice(&(element.len() as u32).to_be_bytes());
            offset += element.len() as u32;
        }
        for element in elements {
            data.extend_from_slice(element);
        }
        data
    }

    fn append_tag(mut profile: Vec<u8>, signature: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let count = u32_at(&profile, 128).unwrap() as usize;
        let end = 132 + count * 12;
        profile.splice(end..end, [0; 12]);
        for index in 0..count {
            let position = 132 + index * 12 + 4;
            let old = u32_at(&profile, position).unwrap();
            profile[position..position + 4].copy_from_slice(&(old + 12).to_be_bytes());
        }
        profile[end..end + 4].copy_from_slice(signature);
        let tag_offset = profile.len() as u32;
        profile[end + 4..end + 8].copy_from_slice(&tag_offset.to_be_bytes());
        profile[end + 8..end + 12].copy_from_slice(&(data.len() as u32).to_be_bytes());
        profile.extend_from_slice(data);
        let size = profile.len() as u32;
        profile[0..4].copy_from_slice(&size.to_be_bytes());
        profile[128..132].copy_from_slice(&((count + 1) as u32).to_be_bytes());
        profile
    }

    #[test]
    fn mpe_matrix_has_precedence_and_direct_absolute_intent() {
        let tag = mpe_tag(3, 3, &[matrix_element(2.0)]);
        let bytes = append_tag(ParsedProfile::new_srgb().encode().unwrap(), b"D2B1", &tag);
        let bytes = append_tag(bytes, b"D2B3", &tag);
        let profile = IccProfile::from_bytes(&bytes).unwrap();
        for intent in [
            IccRenderingIntent::RelativeColorimetric,
            IccRenderingIntent::AbsoluteColorimetric,
        ] {
            let transfer = profile.to_pcs(intent).unwrap();
            assert_eq!(
                transfer.transfer().evaluate(&[0.25, 0.5, 0.75]).unwrap(),
                vec![0.5, 1.0, 1.5]
            );
        }
        let shader = profile
            .to_pcs(IccRenderingIntent::RelativeColorimetric)
            .unwrap()
            .transfer()
            .wgsl(7)
            .unwrap();
        let module = naga::front::wgsl::parse_str(&shader).unwrap();
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap();
    }

    #[test]
    fn b_to_d_uses_float_pcs_without_legacy_lut_encoding() {
        let tag = mpe_tag(3, 3, &[matrix_element(0.5)]);
        let bytes = append_tag(ParsedProfile::new_srgb().encode().unwrap(), b"B2D1", &tag);
        let profile = IccProfile::from_bytes(&bytes).unwrap();
        let output = profile
            .from_pcs(IccRenderingIntent::RelativeColorimetric)
            .unwrap()
            .transfer()
            .evaluate(&[0.25, 0.5, 1.5])
            .unwrap();
        assert_eq!(output, vec![0.125, 0.25, 0.75]);
    }

    #[test]
    fn unsupported_mpe_element_falls_back_to_matrix_trc() {
        let mut element = matrix_element(1.0);
        element[..4].copy_from_slice(b"XXXX");
        let tag = mpe_tag(3, 3, &[element]);
        let bytes = append_tag(ParsedProfile::new_srgb().encode().unwrap(), b"D2B1", &tag);
        let profile = IccProfile::from_bytes(&bytes).unwrap();
        let output = profile
            .to_pcs(IccRenderingIntent::RelativeColorimetric)
            .unwrap()
            .transfer()
            .evaluate(&[1.0, 1.0, 1.0])
            .unwrap();
        assert!((output[1] - 1.0).abs() < 0.01);
    }

    #[test]
    fn segmented_curves_and_float_clut_keep_extended_range() {
        let mut curve = Vec::new();
        curve.extend_from_slice(b"curf");
        curve.extend_from_slice(&[0; 4]);
        curve.extend_from_slice(&1u16.to_be_bytes());
        curve.extend_from_slice(&[0; 2]);
        curve.extend_from_slice(b"parf");
        curve.extend_from_slice(&[0; 4]);
        curve.extend_from_slice(&0u16.to_be_bytes());
        curve.extend_from_slice(&[0; 2]);
        for value in [1.0f32, 1.0, 0.0, 0.0] {
            curve.extend_from_slice(&value.to_be_bytes());
        }
        let mut curve_set = Vec::new();
        curve_set.extend_from_slice(b"cvst");
        curve_set.extend_from_slice(&[0; 4]);
        curve_set.extend_from_slice(&1u16.to_be_bytes());
        curve_set.extend_from_slice(&1u16.to_be_bytes());
        curve_set.extend_from_slice(&20u32.to_be_bytes());
        curve_set.extend_from_slice(&(curve.len() as u32).to_be_bytes());
        curve_set.extend_from_slice(&curve);
        let transfer = parse_mpe(&mpe_tag(1, 1, &[curve_set])).unwrap().unwrap();
        assert_eq!(transfer.evaluate(&[1.5]).unwrap(), vec![1.5]);
        let shader = transfer.wgsl(8).unwrap();
        let module = naga::front::wgsl::parse_str(&shader).unwrap();
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap();

        let mut clut = Vec::new();
        clut.extend_from_slice(b"clut");
        clut.extend_from_slice(&[0; 4]);
        clut.extend_from_slice(&1u16.to_be_bytes());
        clut.extend_from_slice(&1u16.to_be_bytes());
        clut.extend_from_slice(&[2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        clut.extend_from_slice(&(-1.0f32).to_be_bytes());
        clut.extend_from_slice(&(2.0f32).to_be_bytes());
        let transfer = parse_mpe(&mpe_tag(1, 1, &[clut])).unwrap().unwrap();
        assert_eq!(transfer.evaluate(&[0.5]).unwrap(), vec![0.5]);
    }
}
