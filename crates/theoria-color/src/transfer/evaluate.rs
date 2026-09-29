use super::{
    CurveSegment, CurveSegmentKind, ICC_PCS_D50_XYZ, Transfer, TransferClut, TransferCurve,
    TransferCurveKind, TransferError, TransferMatrix, TransferStage,
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

impl CurveSegment {
    pub(crate) fn evaluate(&self, value: f64) -> f64 {
        match &self.kind {
            CurveSegmentKind::Constant(result) => *result,
            CurveSegmentKind::Linear { scale, offset } => scale * value + offset,
            CurveSegmentKind::PowerAffine {
                scale,
                offset,
                exponent,
                output_scale,
                output_offset,
            } => output_scale * (scale * value + offset).powf(*exponent) + output_offset,
            CurveSegmentKind::Logarithmic {
                exponent,
                input_scale,
                input_offset,
                output_scale,
                output_offset,
            } => {
                output_scale * (input_scale * value.powf(*exponent) + input_offset).log10()
                    + output_offset
            }
            CurveSegmentKind::Exponential {
                scale,
                base,
                exponent_scale,
                exponent_offset,
                output_offset,
            } => scale * base.powf(exponent_scale * value + exponent_offset) + output_offset,
            CurveSegmentKind::Sampled { lower, samples } => {
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
        match self.kind() {
            TransferCurveKind::Identity => value,
            TransferCurveKind::Gamma(exponent) => value.clamp(0.0, 1.0).powf(exponent),
            TransferCurveKind::Sampled(values) => {
                let position = value.clamp(0.0, 1.0) * (values.len() - 1) as f64;
                let index = (position.floor() as usize).min(values.len() - 2);
                let amount = position - index as f64;
                values[index] + (values[index + 1] - values[index]) * amount
            }
            TransferCurveKind::Piecewise(segments) => segments
                .iter()
                .find(|segment| {
                    value < segment.upper || (segment.upper_inclusive && value == segment.upper)
                })
                .unwrap_or_else(|| segments.last().unwrap())
                .evaluate(value),
            TransferCurveKind::Clamped {
                curve,
                input,
                output,
            } => curve
                .evaluate(value.clamp(input[0], input[1]))
                .clamp(output[0], output[1]),
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
