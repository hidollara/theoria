use super::{
    CurveKind, MpeSegmentKind, SRGB_DECODE_THRESHOLD, SRGB_ENCODE_THRESHOLD, SRGB_GAMMA,
    SRGB_LINEAR_SCALE, SRGB_POWER_OFFSET, SRGB_POWER_SCALE, Transfer, TransferClut, TransferCurve,
    TransferError, TransferStage,
};

impl Transfer {
    /// Generates one WGSL function named `theoria_transfer_{id}`.
    ///
    /// Both endpoints use fixed-size WGSL arrays so a transfer need not be
    /// limited to three-channel RGB. Numeric data are rounded to `f32` only
    /// here; the CPU evaluator retains the authored `f64` coefficients.
    pub fn wgsl(&self, id: u32) -> Result<String, TransferError> {
        self.lower_wgsl(id, &mut ShaderTables::Inline)
    }

    /// Lowers a transfer to WGSL with all sampled curves and CLUTs packed into
    /// one read-only storage buffer at `(group, binding)`.
    ///
    /// The caller uploads [`TransferShader::table_values`] as contiguous
    /// `f32` values and binds them for the shader stage that calls the generated
    /// function. A transfer without tables needs no binding.
    pub fn wgsl_with_storage(
        &self,
        id: u32,
        group: u32,
        binding: u32,
    ) -> Result<TransferShader, TransferError> {
        let mut tables = ShaderTables::Storage {
            name: format!("theoria_transfer_{id}_tables"),
            group,
            binding,
            values: Vec::new(),
        };
        let source = self.lower_wgsl(id, &mut tables)?;
        let ShaderTables::Storage { values, .. } = tables else {
            unreachable!()
        };
        Ok(TransferShader { source, values })
    }

    fn lower_wgsl(&self, id: u32, tables: &mut ShaderTables) -> Result<String, TransferError> {
        let prefix = format!("theoria_transfer_{id}");
        let mut helpers = String::new();
        let mut body = String::new();
        let mut input_name = String::from("value");
        for (stage_index, stage) in self.stages.iter().enumerate() {
            let output_name = format!("stage_{stage_index}");
            if let TransferStage::Clut(clut) = stage {
                let name = format!("{prefix}_clut_{stage_index}");
                helpers.push_str(&clut.wgsl(&name, tables)?);
                body.push_str(&format!("    let {output_name} = {name}({input_name});\n"));
                input_name = output_name;
                continue;
            }
            if matches!(
                stage,
                TransferStage::XyzD50ToLabD50 | TransferStage::LabD50ToXyzD50
            ) {
                let name = format!("{prefix}_pcs_{stage_index}");
                helpers.push_str(&match stage {
                    TransferStage::XyzD50ToLabD50 => xyz_d50_to_lab_d50_wgsl(&name),
                    TransferStage::LabD50ToXyzD50 => lab_d50_to_xyz_d50_wgsl(&name),
                    _ => unreachable!(),
                });
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
                    for row in 0..matrix.output_channels {
                        let mut expression = wgsl_literal(matrix.offsets[row])?;
                        for column in 0..matrix.input_channels {
                            expression.push_str(&format!(
                                " + {} * {input_name}[{column}]",
                                wgsl_literal(
                                    matrix.coefficients[row * matrix.input_channels + column]
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
                TransferStage::XyzD50ToLabD50 | TransferStage::LabD50ToXyzD50 => {
                    unreachable!("PCS stages are handled above")
                }
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
            self.input_channels,
            self.output_channels
        );
        Ok(source)
    }
}

/// Generated WGSL and the packed storage-buffer contents it expects.
#[derive(Clone, Debug, PartialEq)]
pub struct TransferShader {
    source: String,
    values: Vec<f32>,
}

impl TransferShader {
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Empty when the generated WGSL has no storage-buffer binding.
    pub fn table_values(&self) -> &[f32] {
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
    ) -> Result<(String, usize), TransferError> {
        match self {
            Self::Inline => {
                if inline_limit.is_some_and(|limit| samples.len() > limit) {
                    return Err(TransferError::ShaderTableTooLarge);
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
                                .ok_or(TransferError::NotRepresentableInF32)
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

impl TransferClut {
    fn wgsl(&self, name: &str, tables: &mut ShaderTables) -> Result<String, TransferError> {
        let mut declarations = String::new();
        let (table, offset) = tables.add(
            name,
            &self.values,
            &mut declarations,
            Some(MAX_INLINE_TABLE_VALUES),
        )?;
        let mut source = format!(
            "{declarations}fn {name}(value: array<f32, {}>) -> array<f32, {}> {{\n    var result: array<f32, {}>;\n    for (var corner = 0u; corner < {}u; corner = corner + 1u) {{\n        var index = 0u;\n        var weight = 1.0;\n",
            self.grid.len(),
            self.output_channels,
            self.output_channels,
            1usize << self.grid.len(),
        );
        for (axis, points) in self.grid.iter().copied().enumerate() {
            source.push_str(&format!(
                "        let position_{axis} = clamp(value[{axis}], 0.0, 1.0) * {}.0;\n        let lower_{axis} = min(u32(floor(position_{axis})), {}u);\n        let fraction_{axis} = position_{axis} - f32(lower_{axis});\n        let upper_{axis} = (corner & {}u) != 0u;\n        index = index * {}u + select(lower_{axis}, lower_{axis} + 1u, upper_{axis});\n        weight = weight * select(1.0 - fraction_{axis}, fraction_{axis}, upper_{axis});\n",
                points - 1,
                points - 2,
                1usize << axis,
                points,
            ));
        }
        for channel in 0..self.output_channels {
            source.push_str(&format!(
                "        result[{channel}] = result[{channel}] + weight * {table}[{}u + index * {}u + {channel}u];\n",
                offset, self.output_channels
            ));
        }
        source.push_str("    }\n    return result;\n}\n");
        Ok(source)
    }
}

impl TransferCurve {
    fn wgsl_expression(
        &self,
        source: &str,
        name: &str,
        helpers: &mut String,
        tables: &mut ShaderTables,
    ) -> Result<String, TransferError> {
        match &self.0 {
            CurveKind::Identity => Ok(source.to_owned()),
            CurveKind::Gamma(exponent) => Ok(format!(
                "pow(clamp({source}, 0.0, 1.0), {})",
                wgsl_literal(*exponent)?
            )),
            CurveKind::Sampled(values) => {
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
            CurveKind::IccParametric {
                function,
                parameters,
            } => {
                let p = parameters
                    .iter()
                    .copied()
                    .map(wgsl_literal)
                    .collect::<Result<Vec<_>, _>>()?;
                let (condition, high, low) = match function {
                    0 => (
                        "true".to_owned(),
                        format!("pow(x, {})", p[0]),
                        "0.0".to_owned(),
                    ),
                    1 | 2 => (
                        format!("x >= -{} / {}", p[2], p[1]),
                        format!(
                            "pow({} * x + {}, {}) + {}",
                            p[1],
                            p[2],
                            p[0],
                            if *function == 2 { &p[3] } else { "0.0" }
                        ),
                        if *function == 2 {
                            p[3].clone()
                        } else {
                            "0.0".to_owned()
                        },
                    ),
                    3 | 4 => (
                        format!("x >= {}", p[4]),
                        format!(
                            "pow({} * x + {}, {}) + {}",
                            p[1],
                            p[2],
                            p[0],
                            if *function == 4 { &p[5] } else { "0.0" }
                        ),
                        format!(
                            "{} * x + {}",
                            p[3],
                            if *function == 4 { &p[6] } else { "0.0" }
                        ),
                    ),
                    _ => unreachable!("ICC function type is validated at construction"),
                };
                helpers.push_str(&format!(
                    "fn {name}(value: f32) -> f32 {{\n    let x = clamp(value, 0.0, 1.0);\n    if {condition} {{ return clamp({high}, 0.0, 1.0); }}\n    return clamp({low}, 0.0, 1.0);\n}}\n"
                ));
                Ok(format!("{name}({source})"))
            }
            CurveKind::Mpe(segments) => {
                let mut body = String::new();
                for (index, segment) in segments.iter().enumerate() {
                    let expression = match &segment.kind {
                        MpeSegmentKind::Formula {
                            function,
                            parameters,
                        } => {
                            let p = parameters
                                .iter()
                                .copied()
                                .map(wgsl_literal)
                                .collect::<Result<Vec<_>, _>>()?;
                            match function {
                                0 => {
                                    format!("pow({} * value + {}, {}) + {}", p[1], p[2], p[0], p[3])
                                }
                                1 => format!(
                                    "{} * log({} * pow(value, {}) + {}) / 2.302585092994046 + {}",
                                    p[1], p[2], p[0], p[3], p[4]
                                ),
                                2 => format!(
                                    "{} * pow({}, {} * value + {}) + {}",
                                    p[0], p[1], p[2], p[3], p[4]
                                ),
                                _ => unreachable!(),
                            }
                        }
                        MpeSegmentKind::Sampled { lower, samples } => {
                            let table_name = format!("{name}_segment_{index}");
                            let (table, offset) = tables.add(
                                &table_name,
                                samples,
                                helpers,
                                Some(MAX_INLINE_TABLE_VALUES),
                            )?;
                            let lower = wgsl_literal(*lower)?;
                            let upper = wgsl_literal(segment.upper)?;
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
                    if segment.upper.is_finite() {
                        body.push_str(&format!(
                            "    if value <= {} {{ return {expression}; }}\n",
                            wgsl_literal(segment.upper)?
                        ));
                    } else {
                        body.push_str(&format!("    return {expression};\n"));
                    }
                }
                helpers.push_str(&format!("fn {name}(value: f32) -> f32 {{\n{body}}}\n"));
                Ok(format!("{name}({source})"))
            }
            CurveKind::SrgbDecode | CurveKind::SrgbEncode => {
                let decode = matches!(self.0, CurveKind::SrgbDecode);
                let threshold = if decode {
                    SRGB_DECODE_THRESHOLD
                } else {
                    SRGB_ENCODE_THRESHOLD
                };
                let linear = if decode {
                    format!("value / {}", wgsl_literal(SRGB_LINEAR_SCALE)?)
                } else {
                    format!("value * {}", wgsl_literal(SRGB_LINEAR_SCALE)?)
                };
                let nonlinear = if decode {
                    format!(
                        "sign(value) * pow((magnitude + {}) / {}, {})",
                        wgsl_literal(SRGB_POWER_OFFSET)?,
                        wgsl_literal(SRGB_POWER_SCALE)?,
                        wgsl_literal(SRGB_GAMMA)?,
                    )
                } else {
                    format!(
                        "sign(value) * ({} * pow(magnitude, 1.0 / {}) - {})",
                        wgsl_literal(SRGB_POWER_SCALE)?,
                        wgsl_literal(SRGB_GAMMA)?,
                        wgsl_literal(SRGB_POWER_OFFSET)?,
                    )
                };
                helpers.push_str(&format!(
                    "fn {name}(value: f32) -> f32 {{\n\
                         let magnitude = abs(value);\n\
                         if magnitude <= {} {{ return {linear}; }}\n\
                         return {nonlinear};\n\
                     }}\n",
                    wgsl_literal(threshold)?
                ));
                Ok(format!("{name}({source})"))
            }
        }
    }
}

fn wgsl_literal(value: f64) -> Result<String, TransferError> {
    let rounded = value as f32;
    if !rounded.is_finite() {
        return Err(TransferError::NotRepresentableInF32);
    }
    Ok(format!("{rounded:?}"))
}

fn xyz_d50_to_lab_d50_wgsl(name: &str) -> String {
    format!(
        "fn {name}_f(value: f32) -> f32 {{\n\
             if value > 0.00885645168 {{ return pow(value, 1.0 / 3.0); }}\n\
             return value / 0.12841854935 + 0.13793103448;\n\
         }}\n\
         fn {name}(value: array<f32, 3>) -> array<f32, 3> {{\n\
             let x = {name}_f(value[0] / 0.9642);\n\
             let y = {name}_f(value[1]);\n\
             let z = {name}_f(value[2] / 0.8249);\n\
             return array<f32, 3>(116.0 * y - 16.0, 500.0 * (x - y), 200.0 * (y - z));\n\
         }}\n"
    )
}

fn lab_d50_to_xyz_d50_wgsl(name: &str) -> String {
    format!(
        "fn {name}_inverse(value: f32) -> f32 {{\n\
             if value > 0.20689655172 {{ return value * value * value; }}\n\
             return 0.12841854935 * (value - 0.13793103448);\n\
         }}\n\
         fn {name}(value: array<f32, 3>) -> array<f32, 3> {{\n\
             let y = (value[0] + 16.0) / 116.0;\n\
             let x = y + value[1] / 500.0;\n\
             let z = y - value[2] / 200.0;\n\
             return array<f32, 3>(0.9642 * {name}_inverse(x), {name}_inverse(y), 0.8249 * {name}_inverse(z));\n\
         }}\n"
    )
}
