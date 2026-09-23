use std::{error::Error, fmt, sync::Arc};

use super::{RgbTransfer, RgbTransform, profile_data};

/// A directed sequence of color-coordinate operations.
///
/// An ICC profile can provide different device-to-PCS and PCS-to-device
/// transforms for each rendering intent. A `Transfer` describes one such
/// direction, not an automatically invertible color space. The same stages
/// can be evaluated on the CPU or lowered to WGSL for a renderer.
#[derive(Clone, Debug, PartialEq)]
pub struct Transfer {
    input_channels: usize,
    output_channels: usize,
    stages: Arc<[TransferStage]>,
}

impl Transfer {
    pub fn new(
        input_channels: usize,
        stages: impl Into<Arc<[TransferStage]>>,
    ) -> Result<Self, TransferError> {
        if input_channels == 0 {
            return Err(TransferError::EmptyChannels);
        }
        let stages = stages.into();
        let mut channels = input_channels;
        for stage in stages.iter() {
            if stage.input_channels() != channels {
                return Err(TransferError::ChannelMismatch {
                    expected: channels,
                    actual: stage.input_channels(),
                });
            }
            channels = stage.output_channels();
        }
        Ok(Self {
            input_channels,
            output_channels: channels,
            stages,
        })
    }

    pub const fn input_channels(&self) -> usize {
        self.input_channels
    }

    pub const fn output_channels(&self) -> usize {
        self.output_channels
    }

    pub fn stages(&self) -> &[TransferStage] {
        &self.stages
    }

    /// Connects two directed transfers when their adjacent channel counts match.
    pub fn then(&self, next: &Self) -> Result<Self, TransferError> {
        if self.output_channels != next.input_channels {
            return Err(TransferError::ChannelMismatch {
                expected: self.output_channels,
                actual: next.input_channels,
            });
        }
        Self::new(
            self.input_channels,
            self.stages
                .iter()
                .chain(next.stages.iter())
                .cloned()
                .collect::<Vec<_>>(),
        )
    }

    pub fn evaluate(&self, input: &[f64]) -> Result<Vec<f64>, TransferError> {
        if input.len() != self.input_channels {
            return Err(TransferError::ChannelMismatch {
                expected: self.input_channels,
                actual: input.len(),
            });
        }
        if input.iter().any(|value| !value.is_finite()) {
            return Err(TransferError::NonFiniteValue);
        }
        let mut values = input.to_vec();
        for stage in self.stages.iter() {
            values = match stage {
                TransferStage::Curves(curves) => values
                    .into_iter()
                    .zip(curves.iter())
                    .map(|(value, curve)| curve.evaluate(value))
                    .collect(),
                TransferStage::Matrix(matrix) => matrix.evaluate(&values),
                TransferStage::Clut(clut) => clut.evaluate(&values),
                TransferStage::Clamp(_) => values
                    .into_iter()
                    .map(|value| value.clamp(0.0, 1.0))
                    .collect(),
            };
        }
        if values.iter().any(|value| !value.is_finite()) {
            return Err(TransferError::NonFiniteValue);
        }
        Ok(values)
    }

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

/// A stage in an ordered color transfer. Curve stages preserve channel count;
/// matrices may change it.
#[derive(Clone, Debug, PartialEq)]
pub enum TransferStage {
    Curves(Arc<[TransferCurve]>),
    Matrix(TransferMatrix),
    Clut(TransferClut),
    /// Restricts encoded channels to the ICC curve/CLUT domain.
    Clamp(usize),
}

impl TransferStage {
    pub fn curves(curves: impl Into<Arc<[TransferCurve]>>) -> Result<Self, TransferError> {
        let curves = curves.into();
        if curves.is_empty() {
            return Err(TransferError::EmptyChannels);
        }
        Ok(Self::Curves(curves))
    }

    fn input_channels(&self) -> usize {
        match self {
            Self::Curves(curves) => curves.len(),
            Self::Matrix(matrix) => matrix.input_channels,
            Self::Clut(clut) => clut.grid.len(),
            Self::Clamp(channels) => *channels,
        }
    }

    fn output_channels(&self) -> usize {
        match self {
            Self::Curves(curves) => curves.len(),
            Self::Matrix(matrix) => matrix.output_channels,
            Self::Clut(clut) => clut.output_channels,
            Self::Clamp(channels) => *channels,
        }
    }
}

/// A row-major affine matrix for a transfer stage.
#[derive(Clone, Debug, PartialEq)]
pub struct TransferMatrix {
    input_channels: usize,
    output_channels: usize,
    coefficients: Arc<[f64]>,
    offsets: Arc<[f64]>,
}

impl TransferMatrix {
    pub fn new(
        input_channels: usize,
        output_channels: usize,
        coefficients: impl Into<Arc<[f64]>>,
        offsets: impl Into<Arc<[f64]>>,
    ) -> Result<Self, TransferError> {
        let coefficients = coefficients.into();
        let offsets = offsets.into();
        if input_channels == 0 || output_channels == 0 {
            return Err(TransferError::EmptyChannels);
        }
        if input_channels.checked_mul(output_channels) != Some(coefficients.len())
            || offsets.len() != output_channels
        {
            return Err(TransferError::InvalidMatrixShape);
        }
        if coefficients
            .iter()
            .chain(offsets.iter())
            .any(|value| !value.is_finite())
        {
            return Err(TransferError::NonFiniteValue);
        }
        Ok(Self {
            input_channels,
            output_channels,
            coefficients,
            offsets,
        })
    }

    fn evaluate(&self, input: &[f64]) -> Vec<f64> {
        self.coefficients
            .chunks_exact(self.input_channels)
            .zip(self.offsets.iter())
            .map(|(row, offset)| {
                row.iter()
                    .zip(input.iter())
                    .fold(*offset, |sum, (coefficient, value)| {
                        sum + coefficient * value
                    })
            })
            .collect()
    }
}

/// An ICC-style multidimensional lookup table with normalized samples.
///
/// The first input channel varies least rapidly in `values`; output channels
/// are interleaved at each grid point. Evaluation uses multilinear
/// interpolation. This representation is independent of ICC's 8-/16-bit
/// storage encoding, which a profile decoder must normalize when loading.
#[derive(Clone, Debug, PartialEq)]
pub struct TransferClut {
    grid: Arc<[u8]>,
    output_channels: usize,
    values: Arc<[f64]>,
}

impl TransferClut {
    pub fn new(
        grid: impl Into<Arc<[u8]>>,
        output_channels: usize,
        values: impl Into<Arc<[f64]>>,
    ) -> Result<Self, TransferError> {
        let grid = grid.into();
        let values = values.into();
        if grid.is_empty() || grid.len() > 16 || output_channels == 0 {
            return Err(TransferError::InvalidClutShape);
        }
        let mut entries = Some(output_channels);
        for points in grid.iter().copied() {
            if points < 2 {
                return Err(TransferError::InvalidClutShape);
            }
            entries = entries.and_then(|count| count.checked_mul(usize::from(points)));
        }
        if entries != Some(values.len())
            || values
                .iter()
                .any(|value| !value.is_finite() || !(0.0..=1.0).contains(value))
        {
            return Err(TransferError::InvalidClutShape);
        }
        Ok(Self {
            grid,
            output_channels,
            values,
        })
    }

    pub fn grid(&self) -> &[u8] {
        &self.grid
    }

    pub const fn output_channels(&self) -> usize {
        self.output_channels
    }

    pub fn values(&self) -> &[f64] {
        &self.values
    }

    fn evaluate(&self, input: &[f64]) -> Vec<f64> {
        let mut output = vec![0.0; self.output_channels];
        for corner in 0..(1usize << self.grid.len()) {
            let mut index = 0usize;
            let mut weight = 1.0;
            for (axis, points) in self.grid.iter().copied().enumerate() {
                let position = input[axis].clamp(0.0, 1.0) * f64::from(points - 1);
                let lower = (position.floor() as usize).min(usize::from(points - 2));
                let fraction = position - lower as f64;
                let upper_corner = corner & (1 << axis) != 0;
                index = index * usize::from(points) + lower + usize::from(upper_corner);
                weight *= if upper_corner {
                    fraction
                } else {
                    1.0 - fraction
                };
            }
            let start = index * self.output_channels;
            for (result, value) in output
                .iter_mut()
                .zip(&self.values[start..start + self.output_channels])
            {
                *result += weight * value;
            }
        }
        output
    }

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

/// One-dimensional curves used in ICC-style transfer stages.
///
/// ICC gamma and sampled curves operate on the encoded `[0, 1]` domain.
/// The signed sRGB variants are Theoria's extended-range built-ins, not an
/// assertion that an ICC TRC has that out-of-range behavior.
#[derive(Clone, Debug, PartialEq)]
pub struct TransferCurve(CurveKind);

#[derive(Clone, Debug, PartialEq)]
enum CurveKind {
    Identity,
    Gamma(f64),
    Sampled(Arc<[f64]>),
    IccParametric {
        function: u16,
        parameters: Arc<[f64]>,
    },
    SrgbDecode,
    SrgbEncode,
}

impl TransferCurve {
    pub const IDENTITY: Self = Self(CurveKind::Identity);
    pub const SRGB_DECODE: Self = Self(CurveKind::SrgbDecode);
    pub const SRGB_ENCODE: Self = Self(CurveKind::SrgbEncode);

    pub fn gamma(exponent: f64) -> Result<Self, TransferError> {
        if !exponent.is_finite() || exponent <= 0.0 {
            return Err(TransferError::InvalidCurve);
        }
        Ok(Self(CurveKind::Gamma(exponent)))
    }

    pub fn sampled(values: impl Into<Arc<[f64]>>) -> Result<Self, TransferError> {
        let values = values.into();
        if values.len() < 2
            || values
                .iter()
                .any(|value| !value.is_finite() || !(0.0..=1.0).contains(value))
        {
            return Err(TransferError::InvalidCurve);
        }
        Ok(Self(CurveKind::Sampled(values)))
    }

    /// Creates one of ICC.1's parametric curve functions (types 0 through 4).
    ///
    /// Parameters are in the specification's `g, a, b, c, d, e, f` order;
    /// each function uses only its defined prefix. Values outside the ICC
    /// curve domain and range are clipped to `[0, 1]`.
    pub fn icc_parametric(function: u16, parameters: &[f64]) -> Result<Self, TransferError> {
        let expected = match function {
            0 => 1,
            1 => 3,
            2 => 4,
            3 => 5,
            4 => 7,
            _ => return Err(TransferError::InvalidCurve),
        };
        if parameters.len() != expected
            || parameters.iter().any(|value| !value.is_finite())
            || parameters[0] <= 0.0
            || (function == 1 || function == 2) && parameters[1] <= 0.0
            || (function == 3 || function == 4)
                && (parameters[1] < 0.0
                    || (parameters[4] <= 1.0
                        && parameters[1] * parameters[4].max(0.0) + parameters[2] < 0.0))
        {
            return Err(TransferError::InvalidCurve);
        }
        Ok(Self(CurveKind::IccParametric {
            function,
            parameters: parameters.into(),
        }))
    }

    pub fn evaluate(&self, value: f64) -> f64 {
        match &self.0 {
            CurveKind::Identity => value,
            CurveKind::Gamma(exponent) => value.clamp(0.0, 1.0).powf(*exponent),
            CurveKind::Sampled(values) => {
                let position = value.clamp(0.0, 1.0) * (values.len() - 1) as f64;
                let index = (position.floor() as usize).min(values.len() - 2);
                let amount = position - index as f64;
                values[index] + (values[index + 1] - values[index]) * amount
            }
            CurveKind::IccParametric {
                function,
                parameters,
            } => {
                let x = value.clamp(0.0, 1.0);
                let g = parameters[0];
                let result = match function {
                    0 => x.powf(g),
                    1 | 2 => {
                        let [a, b] = [parameters[1], parameters[2]];
                        if x >= -b / a {
                            (a * x + b).powf(g) + if *function == 2 { parameters[3] } else { 0.0 }
                        } else if *function == 2 {
                            parameters[3]
                        } else {
                            0.0
                        }
                    }
                    3 | 4 => {
                        let [a, b, c, d] =
                            [parameters[1], parameters[2], parameters[3], parameters[4]];
                        if x >= d {
                            (a * x + b).powf(g) + if *function == 4 { parameters[5] } else { 0.0 }
                        } else {
                            c * x + if *function == 4 { parameters[6] } else { 0.0 }
                        }
                    }
                    _ => unreachable!("ICC function type is validated at construction"),
                };
                result.clamp(0.0, 1.0)
            }
            CurveKind::SrgbDecode => RgbTransfer::Srgb.decode_f64(value),
            CurveKind::SrgbEncode => RgbTransfer::Srgb.encode_f64(value),
        }
    }

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
            CurveKind::SrgbDecode | CurveKind::SrgbEncode => {
                let decode = matches!(self.0, CurveKind::SrgbDecode);
                let threshold = if decode {
                    profile_data::SRGB_DECODE_THRESHOLD
                } else {
                    profile_data::SRGB_ENCODE_THRESHOLD
                };
                let linear = if decode {
                    format!("value / {}", wgsl_literal(profile_data::SRGB_LINEAR_SCALE)?)
                } else {
                    format!("value * {}", wgsl_literal(profile_data::SRGB_LINEAR_SCALE)?)
                };
                let nonlinear = if decode {
                    format!(
                        "sign(value) * pow((magnitude + {}) / {}, {})",
                        wgsl_literal(profile_data::SRGB_POWER_OFFSET)?,
                        wgsl_literal(profile_data::SRGB_POWER_SCALE)?,
                        wgsl_literal(profile_data::SRGB_GAMMA)?,
                    )
                } else {
                    format!(
                        "sign(value) * ({} * pow(magnitude, 1.0 / {}) - {})",
                        wgsl_literal(profile_data::SRGB_POWER_SCALE)?,
                        wgsl_literal(profile_data::SRGB_GAMMA)?,
                        wgsl_literal(profile_data::SRGB_POWER_OFFSET)?,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransferError {
    EmptyChannels,
    ChannelMismatch { expected: usize, actual: usize },
    InvalidMatrixShape,
    InvalidClutShape,
    InvalidCurve,
    NonFiniteValue,
    NotRepresentableInF32,
    ShaderTableTooLarge,
}

impl fmt::Display for TransferError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyChannels => formatter.write_str("a transfer must have at least one channel"),
            Self::ChannelMismatch { expected, actual } => {
                write!(formatter, "expected {expected} channels, got {actual}")
            }
            Self::InvalidMatrixShape => {
                formatter.write_str("matrix dimensions do not match its data")
            }
            Self::InvalidClutShape => formatter.write_str("invalid CLUT dimensions or samples"),
            Self::InvalidCurve => formatter.write_str("invalid transfer curve"),
            Self::NonFiniteValue => formatter.write_str("transfer data and values must be finite"),
            Self::NotRepresentableInF32 => {
                formatter.write_str("transfer data cannot be represented by a f32 shader")
            }
            Self::ShaderTableTooLarge => {
                formatter.write_str("sample table requires GPU storage rather than inline WGSL")
            }
        }
    }
}

impl Error for TransferError {}

impl RgbTransform {
    /// Materializes the RGB-to-XYZ D65 direction as a general transfer.
    pub fn to_xyz_d65_transfer(self) -> Result<Transfer, TransferError> {
        let curve = match self.transfer() {
            RgbTransfer::Linear => TransferCurve::IDENTITY,
            RgbTransfer::Srgb => TransferCurve::SRGB_DECODE,
        };
        let stages = [
            TransferStage::curves([curve.clone(), curve.clone(), curve])?,
            TransferStage::Matrix(TransferMatrix::new(
                3,
                3,
                self.to_xyz_d65_matrix()
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>(),
                [0.0; 3],
            )?),
        ];
        Transfer::new(3, stages)
    }

    /// Materializes the XYZ D65-to-RGB direction as a general transfer.
    pub fn xyz_d65_to_rgb_transfer(self) -> Result<Transfer, TransferError> {
        let curve = match self.transfer() {
            RgbTransfer::Linear => TransferCurve::IDENTITY,
            RgbTransfer::Srgb => TransferCurve::SRGB_ENCODE,
        };
        let stages = [
            TransferStage::Matrix(TransferMatrix::new(
                3,
                3,
                self.xyz_d65_to_rgb_matrix()
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>(),
                [0.0; 3],
            )?),
            TransferStage::curves([curve.clone(), curve.clone(), curve])?,
        ];
        Transfer::new(3, stages)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directed_stages_evaluate_in_order_and_change_channel_count() {
        let transfer = Transfer::new(
            3,
            [
                TransferStage::curves([
                    TransferCurve::gamma(2.0).unwrap(),
                    TransferCurve::IDENTITY,
                    TransferCurve::IDENTITY,
                ])
                .unwrap(),
                TransferStage::Matrix(
                    TransferMatrix::new(3, 2, [1.0, 0.5, 0.0, 0.0, 0.0, 1.0], [0.1, 0.0]).unwrap(),
                ),
            ],
        )
        .unwrap();
        assert_eq!(transfer.input_channels(), 3);
        assert_eq!(transfer.output_channels(), 2);
        assert_eq!(
            transfer.evaluate(&[0.5, 0.25, 0.75]).unwrap(),
            vec![0.475, 0.75]
        );
    }

    #[test]
    fn sampled_curves_interpolate_and_clamp_the_icc_domain() {
        let curve = TransferCurve::sampled([0.0, 0.25, 1.0]).unwrap();
        assert_eq!(curve.evaluate(-1.0), 0.0);
        assert_eq!(curve.evaluate(0.25), 0.125);
        assert_eq!(curve.evaluate(0.75), 0.625);
        assert_eq!(curve.evaluate(2.0), 1.0);
    }

    #[test]
    fn multidimensional_clut_uses_icc_channel_order_and_interpolates() {
        let clut = TransferClut::new([2, 2], 1, [0.0, 0.4, 0.6, 1.0]).unwrap();
        let transfer = Transfer::new(2, [TransferStage::Clut(clut)]).unwrap();
        assert_eq!(transfer.evaluate(&[0.0, 1.0]).unwrap(), vec![0.4]);
        assert_eq!(transfer.evaluate(&[1.0, 0.0]).unwrap(), vec![0.6]);
        assert!((transfer.evaluate(&[0.25, 0.75]).unwrap()[0] - 0.45).abs() < 1e-12);
        assert_eq!(transfer.evaluate(&[2.0, -1.0]).unwrap(), vec![0.6]);
    }

    #[test]
    fn clut_wgsl_validates_and_large_tables_report_resource_requirement() {
        let clut = TransferClut::new([2, 2, 2, 2], 3, vec![0.5; 48]).unwrap();
        let transfer = Transfer::new(4, [TransferStage::Clut(clut)]).unwrap();
        let source = transfer.wgsl(11).unwrap();
        let module = naga::front::wgsl::parse_str(&source)
            .unwrap_or_else(|error| panic!("{}", error.emit_to_string(&source)));
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap();

        assert_eq!(
            transfer.evaluate(&[0.2, 0.3, 0.4, 0.5]).unwrap(),
            vec![0.5; 3]
        );

        let large = TransferClut::new([17, 17, 17], 1, vec![0.0; 4913]).unwrap();
        let transfer = Transfer::new(3, [TransferStage::Clut(large)]).unwrap();
        assert_eq!(transfer.wgsl(12), Err(TransferError::ShaderTableTooLarge));

        let shader = transfer.wgsl_with_storage(12, 2, 5).unwrap();
        assert_eq!(shader.table_values().len(), 4913);
        assert!(
            shader
                .source()
                .contains("@group(2) @binding(5) var<storage, read>")
        );
        let module = naga::front::wgsl::parse_str(shader.source())
            .unwrap_or_else(|error| panic!("{}", error.emit_to_string(shader.source())));
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap();

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
    fn storage_shader_packs_multiple_tables_into_one_binding() {
        let first = TransferCurve::sampled([0.0, 0.5, 1.0]).unwrap();
        let second = TransferCurve::sampled([0.0, 1.0]).unwrap();
        let transfer = Transfer::new(2, [TransferStage::curves([first, second]).unwrap()]).unwrap();
        let shader = transfer.wgsl_with_storage(8, 0, 1).unwrap();
        assert_eq!(shader.table_values(), &[0.0, 0.5, 1.0, 0.0, 1.0]);
        assert_eq!(shader.source().matches("@binding(1)").count(), 1);
        assert!(shader.source().contains("[3u + index]"));
        let module = naga::front::wgsl::parse_str(shader.source())
            .unwrap_or_else(|error| panic!("{}", error.emit_to_string(shader.source())));
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap();
    }

    #[test]
    fn icc_parametric_type_four_matches_srgb_decode_within_its_domain() {
        let curve = TransferCurve::icc_parametric(
            4,
            &[
                2.4,
                1.0 / 1.055,
                0.055 / 1.055,
                1.0 / 12.92,
                0.04045,
                0.0,
                0.0,
            ],
        )
        .unwrap();
        for input in [0.0, 0.02, 0.04, 0.05, 0.25, 0.5, 1.0] {
            assert!((curve.evaluate(input) - RgbTransfer::Srgb.decode_f64(input)).abs() < 1e-12);
        }
        assert_eq!(curve.evaluate(-0.5), 0.0);
        assert_eq!(curve.evaluate(1.5), 1.0);
        assert!(TransferCurve::icc_parametric(4, &[1.0; 6]).is_err());
    }

    #[test]
    fn invalid_transfer_data_is_rejected_before_evaluation() {
        assert!(TransferCurve::gamma(0.0).is_err());
        assert!(TransferCurve::sampled([0.0]).is_err());
        assert!(TransferCurve::sampled([0.0, f64::NAN]).is_err());
        assert!(TransferMatrix::new(3, 2, [0.0; 3], [0.0; 2]).is_err());
        assert!(
            Transfer::new(
                2,
                [TransferStage::curves([TransferCurve::IDENTITY]).unwrap()]
            )
            .is_err()
        );
    }

    #[test]
    fn wgsl_lowering_of_the_same_stages_validates() {
        let transfer = Transfer::new(
            4,
            [
                TransferStage::curves([
                    TransferCurve::SRGB_DECODE,
                    TransferCurve::gamma(2.0).unwrap(),
                    TransferCurve::sampled([0.0, 0.3, 1.0]).unwrap(),
                    TransferCurve::icc_parametric(0, &[2.0]).unwrap(),
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
            ],
        )
        .unwrap();
        let source = transfer.wgsl(7).unwrap();
        let module = naga::front::wgsl::parse_str(&source)
            .unwrap_or_else(|error| panic!("{}", error.emit_to_string(&source)));
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap();
        assert!(source.contains("fn theoria_transfer_7("));
        assert!(source.contains("theoria_transfer_7_curve_0_2_table"));
    }

    #[test]
    fn rgb_transform_and_general_transfer_agree_on_both_directions() {
        use crate::color::{DisplayP3, LinearDisplayP3, LinearSrgb, MatrixRgbProfile, Srgb};

        fn check<P: MatrixRgbProfile>() {
            let rgb = [0.23, 0.51, 0.79];
            let xyz = P::TRANSFORM.rgb_to_xyz_d65_f64(rgb);
            let general_xyz = P::TRANSFORM
                .to_xyz_d65_transfer()
                .unwrap()
                .evaluate(&rgb)
                .unwrap();
            for (actual, expected) in general_xyz.into_iter().zip(xyz) {
                assert!((actual - expected).abs() < 0.000_000_1);
            }
            let general_rgb = P::TRANSFORM
                .xyz_d65_to_rgb_transfer()
                .unwrap()
                .evaluate(&xyz)
                .unwrap();
            for (actual, expected) in general_rgb.into_iter().zip(rgb) {
                assert!((actual - expected).abs() < 0.000_01);
            }
        }

        check::<Srgb>();
        check::<LinearSrgb>();
        check::<DisplayP3>();
        check::<LinearDisplayP3>();
    }

    #[test]
    fn transfer_composition_rejects_mismatched_channel_counts() {
        let first = Transfer::new(2, Vec::<TransferStage>::new()).unwrap();
        let second = Transfer::new(3, Vec::<TransferStage>::new()).unwrap();
        assert_eq!(
            first.then(&second),
            Err(TransferError::ChannelMismatch {
                expected: 2,
                actual: 3,
            })
        );
    }
}
