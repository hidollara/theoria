use super::{
    CurveKind, ICC_PCS_D50_XYZ, MpeSegment, MpeSegmentKind, SRGB_DECODE_THRESHOLD,
    SRGB_ENCODE_THRESHOLD, SRGB_GAMMA, SRGB_LINEAR_SCALE, SRGB_POWER_OFFSET, SRGB_POWER_SCALE,
    Transfer, TransferClut, TransferCurve, TransferError, TransferMatrix, TransferStage,
};

impl Transfer {
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
                TransferStage::XyzD50ToLabD50 => xyz_d50_to_lab_d50(&values),
                TransferStage::LabD50ToXyzD50 => lab_d50_to_xyz_d50(&values),
            };
        }
        if values.iter().any(|value| !value.is_finite()) {
            return Err(TransferError::NonFiniteValue);
        }
        Ok(values)
    }
}

impl TransferMatrix {
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

impl TransferClut {
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
}

impl MpeSegment {
    pub(crate) fn evaluate(&self, value: f64) -> f64 {
        match &self.kind {
            MpeSegmentKind::Formula {
                function,
                parameters,
            } => match function {
                0 => (parameters[1] * value + parameters[2]).powf(parameters[0]) + parameters[3],
                1 => {
                    parameters[1]
                        * (parameters[2] * value.powf(parameters[0]) + parameters[3]).log10()
                        + parameters[4]
                }
                2 => {
                    parameters[0] * parameters[1].powf(parameters[2] * value + parameters[3])
                        + parameters[4]
                }
                _ => unreachable!(),
            },
            MpeSegmentKind::Sampled { lower, samples } => {
                let position = (value - lower) / (self.upper - lower) * (samples.len() - 1) as f64;
                let index = (position.floor() as usize).min(samples.len() - 2);
                let amount = position - index as f64;
                samples[index] + (samples[index + 1] - samples[index]) * amount
            }
        }
    }
}

impl TransferCurve {
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
            CurveKind::Mpe(segments) => segments
                .iter()
                .find(|segment| value <= segment.upper)
                .unwrap_or_else(|| segments.last().unwrap())
                .evaluate(value),
            CurveKind::SrgbDecode => decode_srgb_f64(value),
            CurveKind::SrgbEncode => encode_srgb_f64(value),
        }
    }
}

const LAB_DELTA: f64 = 6.0 / 29.0;
const LAB_EPSILON: f64 = LAB_DELTA * LAB_DELTA * LAB_DELTA;

fn lab_f(value: f64) -> f64 {
    if value > LAB_EPSILON {
        value.cbrt()
    } else {
        value / (3.0 * LAB_DELTA * LAB_DELTA) + 4.0 / 29.0
    }
}

fn lab_f_inverse(value: f64) -> f64 {
    if value > LAB_DELTA {
        value * value * value
    } else {
        3.0 * LAB_DELTA * LAB_DELTA * (value - 4.0 / 29.0)
    }
}

fn xyz_to_lab(xyz: [f64; 3], white: [f64; 3]) -> [f64; 3] {
    let x = lab_f(xyz[0] / white[0]);
    let y = lab_f(xyz[1] / white[1]);
    let z = lab_f(xyz[2] / white[2]);
    [116.0 * y - 16.0, 500.0 * (x - y), 200.0 * (y - z)]
}

fn lab_to_xyz(lab: [f64; 3], white: [f64; 3]) -> [f64; 3] {
    let y = (lab[0] + 16.0) / 116.0;
    let x = y + lab[1] / 500.0;
    let z = y - lab[2] / 200.0;
    [
        white[0] * lab_f_inverse(x),
        white[1] * lab_f_inverse(y),
        white[2] * lab_f_inverse(z),
    ]
}

pub(super) fn xyz_d50_to_lab_d50(xyz: &[f64]) -> Vec<f64> {
    xyz_to_lab([xyz[0], xyz[1], xyz[2]], ICC_PCS_D50_XYZ).to_vec()
}

pub(super) fn lab_d50_to_xyz_d50(lab: &[f64]) -> Vec<f64> {
    lab_to_xyz([lab[0], lab[1], lab[2]], ICC_PCS_D50_XYZ).to_vec()
}

pub(super) fn decode_srgb_f64(value: f64) -> f64 {
    let magnitude = value.abs();
    if magnitude <= SRGB_DECODE_THRESHOLD {
        value / SRGB_LINEAR_SCALE
    } else {
        value.signum() * ((magnitude + SRGB_POWER_OFFSET) / SRGB_POWER_SCALE).powf(SRGB_GAMMA)
    }
}

fn encode_srgb_f64(value: f64) -> f64 {
    let magnitude = value.abs();
    if magnitude <= SRGB_ENCODE_THRESHOLD {
        SRGB_LINEAR_SCALE * value
    } else {
        value.signum() * (SRGB_POWER_SCALE * magnitude.powf(1.0 / SRGB_GAMMA) - SRGB_POWER_OFFSET)
    }
}
