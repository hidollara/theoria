use std::{error::Error, fmt, sync::Arc};

mod calculator;
mod evaluate;
#[cfg(test)]
mod tests;

pub use calculator::{CalculatorBinary, CalculatorOperation, CalculatorUnary, TransferCalculator};

/// A directed sequence of color-coordinate operations.
///
/// A `Transfer` describes one direction between coordinate systems, not an
/// automatically invertible color space. ICC profiles can be decoded into
/// this representation, as can transfers defined without a profile. The
/// same stages can be evaluated on the CPU or lowered by a renderer.
#[derive(Clone, Debug, PartialEq)]
pub struct Transfer {
    stages: Vec<TransferStage>,
}

impl Transfer {
    /// The identity for any positive channel count.
    pub const IDENTITY: Self = Self { stages: Vec::new() };

    /// Composes validated transfers in order. An empty sequence is the identity.
    pub fn new(transfers: impl IntoIterator<Item = Self>) -> Result<Self, TransferError> {
        let mut stages: Vec<TransferStage> = Vec::new();
        for mut transfer in transfers {
            if let (Some(previous), Some(next)) = (stages.last(), transfer.stages.first()) {
                if previous.output_channels() != next.input_channels() {
                    return Err(TransferError::ChannelMismatch {
                        expected: previous.output_channels(),
                        actual: next.input_channels(),
                    });
                }
            }
            stages.append(&mut transfer.stages);
        }
        Ok(Self { stages })
    }

    pub fn curves(curves: impl Into<Box<[TransferCurve]>>) -> Result<Self, TransferError> {
        let curves = curves.into();
        if curves.is_empty() {
            return Err(TransferError::EmptyChannels);
        }
        Ok(Self::single(TransferStage::Curves(curves)))
    }

    /// Constructs a matrix whose dimensions are checked by the array types.
    pub fn matrix<const INPUTS: usize, const OUTPUTS: usize>(
        coefficients: [[f64; INPUTS]; OUTPUTS],
        offsets: [f64; OUTPUTS],
    ) -> Result<Self, TransferError> {
        if INPUTS == 0 || OUTPUTS == 0 {
            return Err(TransferError::EmptyChannels);
        }
        Self::matrix_from_parts(
            INPUTS,
            OUTPUTS,
            coefficients
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .into(),
            offsets.into(),
        )
    }

    /// Constructs a matrix from runtime dimensions and row-major coefficients.
    pub fn matrix_from_flat(
        input_channels: usize,
        output_channels: usize,
        coefficients: impl Into<Box<[f64]>>,
        offsets: impl Into<Box<[f64]>>,
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
        Self::matrix_from_parts(input_channels, output_channels, coefficients, offsets)
    }

    pub fn clut(
        grid: impl Into<Box<[u8]>>,
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
        if entries != Some(values.len()) || values.iter().any(|value| !value.is_finite()) {
            return Err(TransferError::InvalidClutShape);
        }
        Ok(Self::single(TransferStage::Clut(TransferClut {
            grid,
            output_channels,
            values,
        })))
    }

    pub fn calculator(
        input_channels: usize,
        output_channels: usize,
        operations: impl Into<Box<[CalculatorOperation]>>,
    ) -> Result<Self, TransferError> {
        Ok(Self::single(TransferStage::Calculator(
            TransferCalculator::new(input_channels, output_channels, operations.into())?,
        )))
    }

    fn matrix_from_parts(
        input_channels: usize,
        output_channels: usize,
        coefficients: Box<[f64]>,
        offsets: Box<[f64]>,
    ) -> Result<Self, TransferError> {
        if coefficients
            .iter()
            .chain(offsets.iter())
            .any(|value| !value.is_finite())
        {
            return Err(TransferError::NonFiniteValue);
        }
        Ok(Self::single(TransferStage::Matrix(TransferMatrix {
            input_channels,
            output_channels,
            coefficients,
            offsets,
        })))
    }

    fn single(stage: TransferStage) -> Self {
        Self {
            stages: vec![stage],
        }
    }

    /// Returns `None` for an empty, channel-polymorphic identity.
    pub fn input_channels(&self) -> Option<usize> {
        self.stages().first().map(TransferStage::input_channels)
    }

    /// Returns `None` for an empty, channel-polymorphic identity.
    pub fn output_channels(&self) -> Option<usize> {
        self.stages().last().map(TransferStage::output_channels)
    }

    pub fn is_empty(&self) -> bool {
        self.stages.is_empty()
    }

    /// Checks this transfer against endpoints whose channel counts are known.
    pub fn supports_channels(&self, input: usize, output: usize) -> bool {
        if input == 0 || output == 0 {
            return false;
        }
        match (self.input_channels(), self.output_channels()) {
            (Some(actual_input), Some(actual_output)) => {
                actual_input == input && actual_output == output
            }
            (None, None) => input == output,
            _ => unreachable!("a transfer has both endpoints or neither"),
        }
    }

    pub fn stages(&self) -> &[TransferStage] {
        &self.stages
    }

    /// Connects two directed transfers when their adjacent channel counts match.
    pub fn then(&self, next: &Self) -> Result<Self, TransferError> {
        Self::new([self.clone(), next.clone()])
    }
}

/// A stage in an ordered color transfer. Curve stages preserve channel count;
/// matrices may change it.
#[derive(Clone, Debug, PartialEq)]
pub enum TransferStage {
    Curves(Box<[TransferCurve]>),
    Matrix(TransferMatrix),
    Clut(TransferClut),
    Calculator(TransferCalculator),
}

impl TransferStage {
    pub fn input_channels(&self) -> usize {
        match self {
            Self::Curves(curves) => curves.len(),
            Self::Matrix(matrix) => matrix.input_channels,
            Self::Clut(clut) => clut.grid.len(),
            Self::Calculator(calculator) => calculator.input_channels(),
        }
    }

    pub fn output_channels(&self) -> usize {
        match self {
            Self::Curves(curves) => curves.len(),
            Self::Matrix(matrix) => matrix.output_channels,
            Self::Clut(clut) => clut.output_channels,
            Self::Calculator(calculator) => calculator.output_channels(),
        }
    }
}

/// A row-major affine matrix for a transfer stage.
#[derive(Clone, Debug, PartialEq)]
pub struct TransferMatrix {
    input_channels: usize,
    output_channels: usize,
    coefficients: Box<[f64]>,
    offsets: Box<[f64]>,
}

impl TransferMatrix {
    pub const fn input_channels(&self) -> usize {
        self.input_channels
    }

    pub const fn output_channels(&self) -> usize {
        self.output_channels
    }

    pub fn coefficients(&self) -> &[f64] {
        &self.coefficients
    }

    pub fn offsets(&self) -> &[f64] {
        &self.offsets
    }
}

/// A multidimensional lookup table.
///
/// The first input channel varies least rapidly in `values`; output channels
/// are interleaved at each grid point. Evaluation uses multilinear
/// interpolation. This representation is independent of source encodings;
/// a profile decoder normalizes its storage when loading. Table outputs may
/// extend beyond the normalized range.
#[derive(Clone, Debug, PartialEq)]
pub struct TransferClut {
    grid: Box<[u8]>,
    output_channels: usize,
    values: Arc<[f64]>,
}

impl TransferClut {
    pub fn grid(&self) -> &[u8] {
        &self.grid
    }

    pub const fn output_channels(&self) -> usize {
        self.output_channels
    }

    pub fn values(&self) -> &[f64] {
        &self.values
    }
}

/// One-dimensional curves in a color transfer.
///
/// Gamma and sampled curves operate on the `[0, 1]` domain. The
/// signed sRGB variants are extended-range built-ins. Piecewise curves can
/// describe wider domains, including curves decoded from ICC profiles.
#[derive(Clone, Debug, PartialEq)]
pub struct TransferCurve(CurveKind);

/// A borrowed description of a validated transfer curve.
#[derive(Clone, Copy, Debug)]
pub enum TransferCurveKind<'a> {
    Identity,
    Gamma(f64),
    Sampled(&'a [f64]),
    Piecewise(&'a [CurveSegment]),
    Clamp { min: f64, max: f64 },
}

#[derive(Clone, Debug, PartialEq)]
enum CurveKind {
    Identity,
    Gamma(f64),
    Sampled(Arc<[f64]>),
    Piecewise(Box<[CurveSegment]>),
    Clamp { min: f64, max: f64 },
}

/// One interval of a piecewise curve, with an explicit upper-boundary rule.
#[derive(Clone, Debug, PartialEq)]
pub struct CurveSegment {
    upper: f64,
    upper_inclusive: bool,
    kind: CurveSegmentKind,
}

/// A numerical operation within a piecewise curve segment.
#[derive(Clone, Debug, PartialEq)]
pub enum CurveSegmentKind {
    Constant(f64),
    Linear {
        scale: f64,
        offset: f64,
    },
    /// `output_scale * (scale * x + offset)^exponent + output_offset`.
    PowerAffine {
        scale: f64,
        offset: f64,
        exponent: f64,
        output_scale: f64,
        output_offset: f64,
    },
    /// `output_scale * log10(input_scale * x^exponent + input_offset) + output_offset`.
    Logarithmic {
        exponent: f64,
        input_scale: f64,
        input_offset: f64,
        output_scale: f64,
        output_offset: f64,
    },
    /// `scale * base^(exponent_scale * x + exponent_offset) + output_offset`.
    Exponential {
        scale: f64,
        base: f64,
        exponent_scale: f64,
        exponent_offset: f64,
        output_offset: f64,
    },
    /// Equally spaced samples over `(lower, segment.upper]`, including the
    /// preceding segment's value at `lower` as the first sample.
    Sampled {
        lower: f64,
        samples: Arc<[f64]>,
    },
}

impl CurveSegment {
    /// Creates a segment that includes its upper boundary.
    pub const fn new(upper: f64, kind: CurveSegmentKind) -> Self {
        Self {
            upper,
            upper_inclusive: true,
            kind,
        }
    }

    /// Creates a segment whose upper boundary belongs to the next segment.
    pub const fn exclusive(upper: f64, kind: CurveSegmentKind) -> Self {
        Self {
            upper,
            upper_inclusive: false,
            kind,
        }
    }

    pub const fn upper(&self) -> f64 {
        self.upper
    }

    pub const fn upper_inclusive(&self) -> bool {
        self.upper_inclusive
    }

    pub const fn kind(&self) -> &CurveSegmentKind {
        &self.kind
    }
}

impl TransferCurve {
    pub const IDENTITY: Self = Self(CurveKind::Identity);

    /// Constructs the extended-range sRGB decoding curve as ordinary segments.
    pub fn srgb_decode() -> Self {
        Self(CurveKind::Piecewise(Box::new(SRGB_DECODE_SEGMENTS.clone())))
    }

    /// Constructs the extended-range sRGB encoding curve as ordinary segments.
    pub fn srgb_encode() -> Self {
        Self(CurveKind::Piecewise(Box::new(SRGB_ENCODE_SEGMENTS.clone())))
    }

    pub fn kind(&self) -> TransferCurveKind<'_> {
        match &self.0 {
            CurveKind::Identity => TransferCurveKind::Identity,
            CurveKind::Gamma(exponent) => TransferCurveKind::Gamma(*exponent),
            CurveKind::Sampled(values) => TransferCurveKind::Sampled(values),
            CurveKind::Piecewise(segments) => TransferCurveKind::Piecewise(segments.as_ref()),
            CurveKind::Clamp { min, max } => TransferCurveKind::Clamp {
                min: *min,
                max: *max,
            },
        }
    }

    /// Whether the segments match the sRGB decoding definition.
    /// Equivalent caller-defined curves also qualify for backend optimization.
    pub fn is_srgb_decode(&self) -> bool {
        matches!(
            &self.0,
            CurveKind::Piecewise(segments)
                if segments.as_ref() == SRGB_DECODE_SEGMENTS.as_slice()
        )
    }

    /// Whether the segments match the sRGB encoding definition.
    pub fn is_srgb_encode(&self) -> bool {
        matches!(
            &self.0,
            CurveKind::Piecewise(segments)
                if segments.as_ref() == SRGB_ENCODE_SEGMENTS.as_slice()
        )
    }

    pub fn piecewise(segments: impl Into<Box<[CurveSegment]>>) -> Result<Self, TransferError> {
        let segments = segments.into();
        if segments.is_empty() {
            return Err(TransferError::InvalidCurve);
        }
        let mut previous = f64::NEG_INFINITY;
        for (index, segment) in segments.iter().enumerate() {
            if (index + 1 == segments.len() && segment.upper != f64::INFINITY)
                || (index + 1 < segments.len() && !segment.upper.is_finite())
                || segment.upper < previous
            {
                return Err(TransferError::InvalidCurve);
            }
            let valid = match &segment.kind {
                CurveSegmentKind::Constant(value) => value.is_finite(),
                CurveSegmentKind::Linear { scale, offset } => {
                    scale.is_finite() && offset.is_finite()
                }
                CurveSegmentKind::PowerAffine {
                    scale,
                    offset,
                    exponent,
                    output_scale,
                    output_offset,
                } => [scale, offset, exponent, output_scale, output_offset]
                    .into_iter()
                    .all(|value| value.is_finite()),
                CurveSegmentKind::Logarithmic {
                    exponent,
                    input_scale,
                    input_offset,
                    output_scale,
                    output_offset,
                } => [
                    exponent,
                    input_scale,
                    input_offset,
                    output_scale,
                    output_offset,
                ]
                .into_iter()
                .all(|value| value.is_finite()),
                CurveSegmentKind::Exponential {
                    scale,
                    base,
                    exponent_scale,
                    exponent_offset,
                    output_offset,
                } => [scale, base, exponent_scale, exponent_offset, output_offset]
                    .into_iter()
                    .all(|value| value.is_finite()),
                CurveSegmentKind::Sampled { lower, samples } => {
                    index > 0
                        && *lower == previous
                        && segment.upper.is_finite()
                        && segment.upper > *lower
                        && samples.len() >= 2
                        && samples.iter().all(|value| value.is_finite())
                }
            };
            if !valid {
                return Err(TransferError::InvalidCurve);
            }
            previous = segment.upper;
        }
        Ok(Self(CurveKind::Piecewise(segments)))
    }

    /// Restricts one channel to inclusive numerical bounds. A negative-infinite
    /// lower bound or positive-infinite upper bound leaves that side unrestricted.
    pub fn clamp(min: f64, max: f64) -> Result<Self, TransferError> {
        if min.is_nan() || max.is_nan() || min > max {
            return Err(TransferError::InvalidCurve);
        }
        Ok(Self(CurveKind::Clamp { min, max }))
    }

    pub fn gamma(exponent: f64) -> Result<Self, TransferError> {
        if !exponent.is_finite() || exponent <= 0.0 {
            return Err(TransferError::InvalidCurve);
        }
        Ok(Self(CurveKind::Gamma(exponent)))
    }

    /// An extended-range power curve that preserves the sign of its input.
    pub fn signed_power(exponent: f64) -> Result<Self, TransferError> {
        if !exponent.is_finite() || exponent <= 0.0 {
            return Err(TransferError::InvalidCurve);
        }
        Self::piecewise([
            CurveSegment::exclusive(
                0.0,
                CurveSegmentKind::PowerAffine {
                    scale: -1.0,
                    offset: 0.0,
                    exponent,
                    output_scale: -1.0,
                    output_offset: 0.0,
                },
            ),
            CurveSegment::new(
                f64::INFINITY,
                CurveSegmentKind::PowerAffine {
                    scale: 1.0,
                    offset: 0.0,
                    exponent,
                    output_scale: 1.0,
                    output_offset: 0.0,
                },
            ),
        ])
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
}

/// ICC D50 PCS white, expressed as the specified XYZ tristimulus values.
pub(super) const ICC_PCS_D50_XYZ: [f64; 3] = [0.9642, 1.0, 0.8249];
// The built-in names retain their identity as optimization hints, but use the
// same piecewise representation as curves supplied by callers or ICC profiles.
const SRGB_DECODE_THRESHOLD: f64 = 0.04045;
const SRGB_ENCODE_THRESHOLD: f64 = 0.003_130_8;
const SRGB_LINEAR_SCALE: f64 = 12.92;
const SRGB_POWER_SCALE: f64 = 1.055;
const SRGB_POWER_OFFSET: f64 = 0.055;
const SRGB_GAMMA: f64 = 2.4;

static SRGB_DECODE_SEGMENTS: [CurveSegment; 3] = [
    CurveSegment::exclusive(
        -SRGB_DECODE_THRESHOLD,
        CurveSegmentKind::PowerAffine {
            scale: -1.0 / SRGB_POWER_SCALE,
            offset: SRGB_POWER_OFFSET / SRGB_POWER_SCALE,
            exponent: SRGB_GAMMA,
            output_scale: -1.0,
            output_offset: 0.0,
        },
    ),
    CurveSegment::new(
        SRGB_DECODE_THRESHOLD,
        CurveSegmentKind::Linear {
            scale: 1.0 / SRGB_LINEAR_SCALE,
            offset: 0.0,
        },
    ),
    CurveSegment::new(
        f64::INFINITY,
        CurveSegmentKind::PowerAffine {
            scale: 1.0 / SRGB_POWER_SCALE,
            offset: SRGB_POWER_OFFSET / SRGB_POWER_SCALE,
            exponent: SRGB_GAMMA,
            output_scale: 1.0,
            output_offset: 0.0,
        },
    ),
];

static SRGB_ENCODE_SEGMENTS: [CurveSegment; 3] = [
    CurveSegment::exclusive(
        -SRGB_ENCODE_THRESHOLD,
        CurveSegmentKind::PowerAffine {
            scale: -1.0,
            offset: 0.0,
            exponent: 1.0 / SRGB_GAMMA,
            output_scale: -SRGB_POWER_SCALE,
            output_offset: SRGB_POWER_OFFSET,
        },
    ),
    CurveSegment::new(
        SRGB_ENCODE_THRESHOLD,
        CurveSegmentKind::Linear {
            scale: SRGB_LINEAR_SCALE,
            offset: 0.0,
        },
    ),
    CurveSegment::new(
        f64::INFINITY,
        CurveSegmentKind::PowerAffine {
            scale: 1.0,
            offset: 0.0,
            exponent: 1.0 / SRGB_GAMMA,
            output_scale: SRGB_POWER_SCALE,
            output_offset: -SRGB_POWER_OFFSET,
        },
    ),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransferError {
    EmptyChannels,
    ChannelMismatch { expected: usize, actual: usize },
    InvalidMatrixShape,
    InvalidClutShape,
    InvalidCurve,
    InvalidCalculator,
    NonFiniteValue,
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
            Self::InvalidCalculator => {
                formatter.write_str("invalid calculator input, stack or branch structure")
            }
            Self::NonFiniteValue => formatter.write_str("transfer data and values must be finite"),
        }
    }
}

impl Error for TransferError {}
