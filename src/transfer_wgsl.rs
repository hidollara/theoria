use std::{error::Error, fmt};

use theoria_color::{
    CurveSegmentKind, Transfer, TransferClut, TransferCurve, TransferCurveKind, TransferStage,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TransferWgslError {
    NotRepresentableInF32,
    ShaderTableTooLarge,
}

impl fmt::Display for TransferWgslError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotRepresentableInF32 => {
                formatter.write_str("transfer data cannot be represented by a f32 shader")
            }
            Self::ShaderTableTooLarge => {
                formatter.write_str("sample table requires GPU storage rather than inline WGSL")
            }
        }
    }
}

impl Error for TransferWgslError {}

pub(crate) trait TransferWgslExt {
    fn wgsl(&self, id: u32) -> Result<String, TransferWgslError>;
    fn wgsl_with_storage(
        &self,
        id: u32,
        group: u32,
        binding: u32,
    ) -> Result<TransferShader, TransferWgslError>;
}

impl TransferWgslExt for Transfer {
    /// Generates one WGSL function named `theoria_transfer_{id}`.
    /// An empty transfer needs no function and produces an empty string.
    ///
    /// Both endpoints use fixed-size WGSL arrays so a transfer need not be
    /// limited to three-channel RGB. Numeric data are rounded to `f32` only
    /// here; the CPU evaluator retains the authored `f64` coefficients.
    fn wgsl(&self, id: u32) -> Result<String, TransferWgslError> {
        lower_wgsl(self, id, &mut ShaderTables::Inline)
    }

    /// Lowers a transfer to WGSL with all sampled curves and CLUTs packed into
    /// one read-only storage buffer at `(group, binding)`.
    ///
    /// The caller uploads [`TransferShader::table_values`] as contiguous
    /// `f32` values and binds them for the shader stage that calls the generated
    /// function. A transfer without tables needs no binding.
    fn wgsl_with_storage(
        &self,
        id: u32,
        group: u32,
        binding: u32,
    ) -> Result<TransferShader, TransferWgslError> {
        let mut tables = ShaderTables::Storage {
            name: format!("theoria_transfer_{id}_tables"),
            group,
            binding,
            values: Vec::new(),
        };
        let source = lower_wgsl(self, id, &mut tables)?;
        let ShaderTables::Storage { values, .. } = tables else {
            unreachable!()
        };
        Ok(TransferShader { source, values })
    }
}

fn lower_wgsl(
    transfer: &Transfer,
    id: u32,
    tables: &mut ShaderTables,
) -> Result<String, TransferWgslError> {
    if transfer.is_empty() {
        return Ok(String::new());
    }
    let prefix = format!("theoria_transfer_{id}");
    let mut helpers = String::new();
    let mut body = String::new();
    let mut input_name = String::from("value");
    for (stage_index, stage) in transfer.stages().iter().enumerate() {
        let output_name = format!("stage_{stage_index}");
        if let TransferStage::Clut(clut) = stage {
            let name = format!("{prefix}_clut_{stage_index}");
            helpers.push_str(&clut.wgsl(&name, tables)?);
            body.push_str(&format!("    let {output_name} = {name}({input_name});\n"));
            input_name = output_name;
            continue;
        }
        let mut expressions = Vec::with_capacity(stage.output_channels());
        match stage {
            TransferStage::Curves(curves) => {
                for (channel, curve) in curves.iter().enumerate() {
                    let source = format!("{input_name}[{channel}]");
                    expressions.push(curve.wgsl_expression(
                        &source,
                        &format!("{prefix}_curve_{stage_index}_{channel}"),
                        &mut helpers,
                        tables,
                    )?);
                }
            }
            TransferStage::Matrix(matrix) => {
                for row in 0..matrix.output_channels() {
                    let mut expression = wgsl_literal(matrix.offsets()[row])?;
                    for column in 0..matrix.input_channels() {
                        expression.push_str(&format!(
                            " + {} * {input_name}[{column}]",
                            wgsl_literal(
                                matrix.coefficients()[row * matrix.input_channels() + column]
                            )?
                        ));
                    }
                    expressions.push(expression);
                }
            }
            TransferStage::Clamp(channels) => {
                for channel in 0..*channels {
                    expressions.push(format!("clamp({input_name}[{channel}], 0.0, 1.0)"));
                }
            }
            TransferStage::Clut(_) => unreachable!("CLUT stages are handled above"),
        }
        body.push_str(&format!(
            "    let {output_name} = array<f32, {}>({});\n",
            stage.output_channels(),
            expressions.join(", ")
        ));
        input_name = output_name;
    }
    let source = format!(
        "{}{helpers}\nfn {prefix}(value: array<f32, {}>) -> array<f32, {}> {{\n{body}    return {input_name};\n}}\n",
        tables.declaration(),
        transfer.input_channels().expect("nonempty transfer"),
        transfer.output_channels().expect("nonempty transfer")
    );
    Ok(source)
}

/// Generated WGSL and the packed storage-buffer contents it expects.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TransferShader {
    source: String,
    values: Vec<f32>,
}

impl TransferShader {
    pub(crate) fn source(&self) -> &str {
        &self.source
    }

    /// Empty when the generated WGSL has no storage-buffer binding.
    pub(crate) fn table_values(&self) -> &[f32] {
        &self.values
    }
}

enum ShaderTables {
    Inline,
    Storage {
        name: String,
        group: u32,
        binding: u32,
        values: Vec<f32>,
    },
}

const MAX_INLINE_TABLE_VALUES: usize = 4096;

impl ShaderTables {
    fn add(
        &mut self,
        name: &str,
        samples: &[f64],
        helpers: &mut String,
        inline_limit: Option<usize>,
    ) -> Result<(String, usize), TransferWgslError> {
        match self {
            Self::Inline => {
                if inline_limit.is_some_and(|limit| samples.len() > limit) {
                    return Err(TransferWgslError::ShaderTableTooLarge);
                }
                let literals = samples
                    .iter()
                    .copied()
                    .map(wgsl_literal)
                    .collect::<Result<Vec<_>, _>>()?;
                let table_name = format!("{name}_table");
                helpers.push_str(&format!(
                    "const {table_name} = array<f32, {}>({});\n",
                    samples.len(),
                    literals.join(", ")
                ));
                Ok((table_name, 0))
            }
            Self::Storage { name, values, .. } => {
                let offset = values.len();
                values.extend(
                    samples
                        .iter()
                        .copied()
                        .map(|sample| {
                            let rounded = sample as f32;
                            rounded
                                .is_finite()
                                .then_some(rounded)
                                .ok_or(TransferWgslError::NotRepresentableInF32)
                        })
                        .collect::<Result<Vec<_>, _>>()?,
                );
                Ok((name.clone(), offset))
            }
        }
    }

    fn declaration(&self) -> String {
        match self {
            Self::Storage {
                name,
                group,
                binding,
                values,
            } if !values.is_empty() => format!(
                "@group({group}) @binding({binding}) var<storage, read> {name}: array<f32>;\n"
            ),
            _ => String::new(),
        }
    }
}

trait ClutWgslExt {
    fn wgsl(&self, name: &str, tables: &mut ShaderTables) -> Result<String, TransferWgslError>;
}

impl ClutWgslExt for TransferClut {
    fn wgsl(&self, name: &str, tables: &mut ShaderTables) -> Result<String, TransferWgslError> {
        let mut declarations = String::new();
        let (table, offset) = tables.add(
            name,
            self.values(),
            &mut declarations,
            Some(MAX_INLINE_TABLE_VALUES),
        )?;
        let mut source = format!(
            "{declarations}fn {name}(value: array<f32, {}>) -> array<f32, {}> {{\n    var result: array<f32, {}>;\n    for (var corner = 0u; corner < {}u; corner = corner + 1u) {{\n        var index = 0u;\n        var weight = 1.0;\n",
            self.grid().len(),
            self.output_channels(),
            self.output_channels(),
            1usize << self.grid().len(),
        );
        for (axis, points) in self.grid().iter().copied().enumerate() {
            source.push_str(&format!(
                "        let position_{axis} = clamp(value[{axis}], 0.0, 1.0) * {}.0;\n        let lower_{axis} = min(u32(floor(position_{axis})), {}u);\n        let fraction_{axis} = position_{axis} - f32(lower_{axis});\n        let upper_{axis} = (corner & {}u) != 0u;\n        index = index * {}u + select(lower_{axis}, lower_{axis} + 1u, upper_{axis});\n        weight = weight * select(1.0 - fraction_{axis}, fraction_{axis}, upper_{axis});\n",
                points - 1,
                points - 2,
                1usize << axis,
                points,
            ));
        }
        for channel in 0..self.output_channels() {
            source.push_str(&format!(
                "        result[{channel}] = result[{channel}] + weight * {table}[{}u + index * {}u + {channel}u];\n",
                offset, self.output_channels()
            ));
        }
        source.push_str("    }\n    return result;\n}\n");
        Ok(source)
    }
}

trait CurveWgslExt {
    fn wgsl_expression(
        &self,
        source: &str,
        name: &str,
        helpers: &mut String,
        tables: &mut ShaderTables,
    ) -> Result<String, TransferWgslError>;
}

impl CurveWgslExt for TransferCurve {
    fn wgsl_expression(
        &self,
        source: &str,
        name: &str,
        helpers: &mut String,
        tables: &mut ShaderTables,
    ) -> Result<String, TransferWgslError> {
        match self.kind() {
            TransferCurveKind::Identity => Ok(source.to_owned()),
            TransferCurveKind::Gamma(exponent) => Ok(format!(
                "pow(clamp({source}, 0.0, 1.0), {})",
                wgsl_literal(exponent)?
            )),
            TransferCurveKind::Sampled(values) => {
                let (table, offset) =
                    tables.add(name, values, helpers, Some(MAX_INLINE_TABLE_VALUES))?;
                helpers.push_str(&format!(
                    "fn {name}(value: f32) -> f32 {{\n\
                         let position = clamp(value, 0.0, 1.0) * {}.0;\n\
                         let index = min(u32(floor(position)), {}u);\n\
                         return mix({table}[{}u + index], {table}[{}u + index + 1u], position - f32(index));\n\
                     }}\n",
                    values.len() - 1,
                    values.len() - 2,
                    offset,
                    offset,
                ));
                Ok(format!("{name}({source})"))
            }
            TransferCurveKind::Piecewise(segments) => {
                let mut body = String::new();
                for (index, segment) in segments.iter().enumerate() {
                    let expression = match segment.kind() {
                        CurveSegmentKind::Constant(value) => wgsl_literal(*value)?,
                        CurveSegmentKind::Linear { scale, offset } => format!(
                            "{} * value + {}",
                            wgsl_literal(*scale)?,
                            wgsl_literal(*offset)?
                        ),
                        CurveSegmentKind::PowerAffine {
                            scale,
                            offset,
                            exponent,
                            output_scale,
                            output_offset,
                        } => format!(
                            "{} * pow({} * value + {}, {}) + {}",
                            wgsl_literal(*output_scale)?,
                            wgsl_literal(*scale)?,
                            wgsl_literal(*offset)?,
                            wgsl_literal(*exponent)?,
                            wgsl_literal(*output_offset)?
                        ),
                        CurveSegmentKind::Logarithmic {
                            exponent,
                            input_scale,
                            input_offset,
                            output_scale,
                            output_offset,
                        } => format!(
                            "{} * log({} * pow(value, {}) + {}) / 2.302585092994046 + {}",
                            wgsl_literal(*output_scale)?,
                            wgsl_literal(*input_scale)?,
                            wgsl_literal(*exponent)?,
                            wgsl_literal(*input_offset)?,
                            wgsl_literal(*output_offset)?
                        ),
                        CurveSegmentKind::Exponential {
                            scale,
                            base,
                            exponent_scale,
                            exponent_offset,
                            output_offset,
                        } => format!(
                            "{} * pow({}, {} * value + {}) + {}",
                            wgsl_literal(*scale)?,
                            wgsl_literal(*base)?,
                            wgsl_literal(*exponent_scale)?,
                            wgsl_literal(*exponent_offset)?,
                            wgsl_literal(*output_offset)?
                        ),
                        CurveSegmentKind::Sampled { lower, samples } => {
                            let table_name = format!("{name}_segment_{index}");
                            let (table, offset) = tables.add(
                                &table_name,
                                samples,
                                helpers,
                                Some(MAX_INLINE_TABLE_VALUES),
                            )?;
                            let lower = wgsl_literal(*lower)?;
                            let upper = wgsl_literal(segment.upper())?;
                            let count = samples.len() - 1;
                            helpers.push_str(&format!(
                                "fn {table_name}_read(value: f32) -> f32 {{\n\
                                 let position = clamp((value - {lower}) / ({upper} - {lower}) * {count}.0, 0.0, {count}.0);\n\
                                 let index = min(u32(floor(position)), {}u);\n\
                                 return mix({table}[{}u + index], {table}[{}u + index + 1u], position - f32(index));\n\
                                 }}\n",
                                count - 1,
                                offset,
                                offset,
                            ));
                            format!("{table_name}_read(value)")
                        }
                    };
                    if segment.upper().is_finite() {
                        let comparison = if segment.upper_inclusive() { "<=" } else { "<" };
                        body.push_str(&format!(
                            "    if value {comparison} {} {{ return {expression}; }}\n",
                            wgsl_literal(segment.upper())?
                        ));
                    } else {
                        body.push_str(&format!("    return {expression};\n"));
                    }
                }
                helpers.push_str(&format!("fn {name}(value: f32) -> f32 {{\n{body}}}\n"));
                Ok(format!("{name}({source})"))
            }
            TransferCurveKind::Clamped {
                curve,
                input,
                output,
            } => {
                let input = format!(
                    "clamp({source}, {}, {})",
                    wgsl_literal(input[0])?,
                    wgsl_literal(input[1])?
                );
                let expression = curve.wgsl_expression(&input, name, helpers, tables)?;
                Ok(format!(
                    "clamp({expression}, {}, {})",
                    wgsl_literal(output[0])?,
                    wgsl_literal(output[1])?
                ))
            }
        }
    }
}

fn wgsl_literal(value: f64) -> Result<String, TransferWgslError> {
    let rounded = value as f32;
    if !rounded.is_finite() {
        return Err(TransferWgslError::NotRepresentableInF32);
    }
    Ok(format!("{rounded:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use theoria_color::{
        ColorSpace, CurveSegment, IccProfile, IccRenderingIntent, LabD50, LinearSrgb, Srgb,
        TransferMatrix, XyzD50,
    };

    fn validate(source: &str) {
        let module = naga::front::wgsl::parse_str(source)
            .unwrap_or_else(|error| panic!("{}", error.emit_to_string(source)));
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap();
    }

    #[test]
    fn pcs_stages_generate_valid_wgsl() {
        let xyz = ColorSpace::from(XyzD50);
        let lab = ColorSpace::from(LabD50);
        let transfer = xyz
            .transfer_to(&lab)
            .unwrap()
            .then(&lab.transfer_to(&xyz).unwrap())
            .unwrap();
        let source = transfer.wgsl(51).unwrap();
        validate(&source);
        assert!(source.contains("pow("));
        assert!(source.contains("theoria_transfer_51"));
    }

    #[test]
    fn curves_and_matrix_generate_valid_wgsl() {
        let transfer = Transfer::new([
            TransferStage::curves([
                TransferCurve::IDENTITY,
                TransferCurve::gamma(2.0).unwrap(),
                TransferCurve::sampled([0.0, 0.3, 1.0]).unwrap(),
                TransferCurve::clamped(
                    TransferCurve::piecewise([
                        CurveSegment::exclusive(0.5, CurveSegmentKind::Constant(0.0)),
                        CurveSegment::new(
                            f64::INFINITY,
                            CurveSegmentKind::PowerAffine {
                                scale: 1.0,
                                offset: 0.0,
                                exponent: 2.0,
                                output_scale: 1.0,
                                output_offset: 0.0,
                            },
                        ),
                    ])
                    .unwrap(),
                    [0.0, 1.0],
                    [0.0, 1.0],
                )
                .unwrap(),
            ])
            .unwrap(),
            TransferStage::Matrix(
                TransferMatrix::new(
                    4,
                    3,
                    [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0],
                    [0.0; 3],
                )
                .unwrap(),
            ),
        ])
        .unwrap();
        let source = transfer.wgsl(7).unwrap();
        validate(&source);
        assert!(source.contains("fn theoria_transfer_7("));
        assert!(source.contains("theoria_transfer_7_curve_0_2_table"));
        assert!(source.contains("if value < 0.5"));
    }

    #[test]
    fn empty_transfer_needs_no_shader_function_or_table() {
        let identity = Transfer::IDENTITY;
        assert!(identity.wgsl(7).unwrap().is_empty());
        let shader = identity.wgsl_with_storage(7, 0, 0).unwrap();
        assert!(shader.source().is_empty());
        assert!(shader.table_values().is_empty());
    }

    #[test]
    fn logarithmic_and_exponential_segments_generate_valid_wgsl() {
        let transfer = Transfer::new([TransferStage::curves([
            TransferCurve::piecewise([CurveSegment::new(
                f64::INFINITY,
                CurveSegmentKind::Logarithmic {
                    exponent: 1.0,
                    input_scale: 1.0,
                    input_offset: 1.0,
                    output_scale: 1.0,
                    output_offset: 0.0,
                },
            )])
            .unwrap(),
            TransferCurve::piecewise([CurveSegment::new(
                f64::INFINITY,
                CurveSegmentKind::Exponential {
                    scale: 1.0,
                    base: 2.0,
                    exponent_scale: 1.0,
                    exponent_offset: 0.0,
                    output_offset: 0.0,
                },
            )])
            .unwrap(),
        ])
        .unwrap()])
        .unwrap();
        validate(&transfer.wgsl(13).unwrap());
    }

    #[test]
    fn named_srgb_curves_lower_through_piecewise_wgsl() {
        let decode = ColorSpace::from(Srgb).transfer_to(LinearSrgb).unwrap();
        let encode = ColorSpace::from(LinearSrgb).transfer_to(Srgb).unwrap();
        for (index, transfer, threshold) in [(14, decode, "-0.04045"), (15, encode, "-0.0031308")] {
            let shader = transfer.wgsl(index).unwrap();
            validate(&shader);
            assert!(shader.contains(&format!("if value < {threshold}")));
            assert!(!shader.contains("sign(value)"));
        }
    }

    #[test]
    fn clut_uses_storage_when_inline_table_is_too_large() {
        let clut = TransferClut::new([2, 2, 2, 2], 3, vec![0.5; 48]).unwrap();
        let transfer = Transfer::new([TransferStage::Clut(clut)]).unwrap();
        validate(&transfer.wgsl(11).unwrap());

        let large = TransferClut::new([17, 17, 17], 1, vec![0.0; 4913]).unwrap();
        let transfer = Transfer::new([TransferStage::Clut(large)]).unwrap();
        assert_eq!(
            transfer.wgsl(12),
            Err(TransferWgslError::ShaderTableTooLarge)
        );
        let shader = transfer.wgsl_with_storage(12, 2, 5).unwrap();
        assert_eq!(shader.table_values().len(), 4913);
        assert!(
            shader
                .source()
                .contains("@group(2) @binding(5) var<storage, read>")
        );
        validate(shader.source());

        let fragment = format!(
            "{}\n@fragment fn fs_main() -> @location(0) vec4<f32> {{\n    let value = theoria_transfer_12(array<f32, 3>(0.2, 0.3, 0.4));\n    return vec4<f32>(value[0], value[0], value[0], 1.0);\n}}",
            shader.source()
        );
        let module = naga::front::wgsl::parse_str(&fragment)
            .unwrap_or_else(|error| panic!("{}", error.emit_to_string(&fragment)));
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap();
        let pipeline = naga::back::spv::PipelineOptions {
            entry_point: "fs_main".into(),
            shader_stage: naga::ShaderStage::Fragment,
        };
        assert!(
            !naga::back::spv::write_vec(
                &module,
                &info,
                &naga::back::spv::Options::default(),
                Some(&pipeline),
            )
            .unwrap()
            .is_empty()
        );
    }

    #[test]
    fn storage_shader_packs_curves_into_one_binding() {
        let first = TransferCurve::sampled([0.0, 0.5, 1.0]).unwrap();
        let second = TransferCurve::sampled([0.0, 1.0]).unwrap();
        let transfer = Transfer::new([TransferStage::curves([first, second]).unwrap()]).unwrap();
        let shader = transfer.wgsl_with_storage(8, 0, 1).unwrap();
        assert_eq!(shader.table_values(), &[0.0, 0.5, 1.0, 0.0, 1.0]);
        assert_eq!(shader.source().matches("@binding(1)").count(), 1);
        assert!(shader.source().contains("[3u + index]"));
        validate(shader.source());
    }

    #[test]
    fn icc_profile_transfer_generates_valid_wgsl() {
        let bytes = include_bytes!("../crates/theoria-color/src/color/fixtures/sRGB-v4.icc");
        let profile = IccProfile::from_bytes(bytes).unwrap();
        let space = profile.color_space(IccRenderingIntent::RelativeColorimetric);
        let transfer = space.space_to_pcs().unwrap();
        let shader = transfer.wgsl_with_storage(22, 1, 4).unwrap();
        validate(shader.source());
    }

    #[test]
    fn icc_mpe_segmented_curves_generate_valid_wgsl() {
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
        curve_set.extend_from_slice(&3u16.to_be_bytes());
        curve_set.extend_from_slice(&3u16.to_be_bytes());
        for channel in 0..3 {
            curve_set.extend_from_slice(&(36 + channel * curve.len() as u32).to_be_bytes());
            curve_set.extend_from_slice(&(curve.len() as u32).to_be_bytes());
        }
        for _ in 0..3 {
            curve_set.extend_from_slice(&curve);
        }

        let mut tag = Vec::new();
        tag.extend_from_slice(b"mpet");
        tag.extend_from_slice(&[0; 4]);
        tag.extend_from_slice(&3u16.to_be_bytes());
        tag.extend_from_slice(&3u16.to_be_bytes());
        tag.extend_from_slice(&1u32.to_be_bytes());
        tag.extend_from_slice(&24u32.to_be_bytes());
        tag.extend_from_slice(&(curve_set.len() as u32).to_be_bytes());
        tag.extend_from_slice(&curve_set);

        let mut bytes = moxcms::ColorProfile::new_srgb().encode().unwrap();
        let count = u32::from_be_bytes(bytes[128..132].try_into().unwrap()) as usize;
        let table_end = 132 + count * 12;
        bytes.splice(table_end..table_end, [0; 12]);
        for index in 0..count {
            let position = 132 + index * 12 + 4;
            let offset = u32::from_be_bytes(bytes[position..position + 4].try_into().unwrap());
            bytes[position..position + 4].copy_from_slice(&(offset + 12).to_be_bytes());
        }
        bytes[table_end..table_end + 4].copy_from_slice(b"D2B1");
        let tag_offset = bytes.len() as u32;
        bytes[table_end + 4..table_end + 8].copy_from_slice(&tag_offset.to_be_bytes());
        bytes[table_end + 8..table_end + 12].copy_from_slice(&(tag.len() as u32).to_be_bytes());
        bytes.extend_from_slice(&tag);
        let profile_size = bytes.len() as u32;
        bytes[0..4].copy_from_slice(&profile_size.to_be_bytes());
        bytes[128..132].copy_from_slice(&((count + 1) as u32).to_be_bytes());

        let profile = IccProfile::from_bytes(&bytes).unwrap();
        let transfer = profile
            .color_space(IccRenderingIntent::RelativeColorimetric)
            .space_to_pcs()
            .unwrap();
        assert!(matches!(
            &transfer.stages()[0],
            TransferStage::Curves(curves)
                if matches!(curves[0].kind(), TransferCurveKind::Piecewise(_))
        ));
        validate(&transfer.wgsl(8).unwrap());
    }
}
