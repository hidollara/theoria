use std::{error::Error, fmt, sync::Arc};

mod evaluate;
#[cfg(test)]
mod tests;

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
    /// Converts between actual ICC D50 PCS coordinates (not LUT encodings).
    XyzD50ToLabD50,
    /// Converts between actual ICC D50 PCS coordinates (not LUT encodings).
    LabD50ToXyzD50,
}

impl TransferStage {
    pub fn curves(curves: impl Into<Arc<[TransferCurve]>>) -> Result<Self, TransferError> {
        let curves = curves.into();
        if curves.is_empty() {
            return Err(TransferError::EmptyChannels);
        }
        Ok(Self::Curves(curves))
    }

    pub fn input_channels(&self) -> usize {
        match self {
            Self::Curves(curves) => curves.len(),
            Self::Matrix(matrix) => matrix.input_channels,
            Self::Clut(clut) => clut.grid.len(),
            Self::Clamp(channels) => *channels,
            Self::XyzD50ToLabD50 | Self::LabD50ToXyzD50 => 3,
        }
    }

    pub fn output_channels(&self) -> usize {
        match self {
            Self::Curves(curves) => curves.len(),
            Self::Matrix(matrix) => matrix.output_channels,
            Self::Clut(clut) => clut.output_channels,
            Self::Clamp(channels) => *channels,
            Self::XyzD50ToLabD50 | Self::LabD50ToXyzD50 => 3,
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

/// An ICC-style multidimensional lookup table.
///
/// The first input channel varies least rapidly in `values`; output channels
/// are interleaved at each grid point. Evaluation uses multilinear
/// interpolation. This representation is independent of ICC's 8-/16-bit
/// storage encoding, which a profile decoder must normalize when loading.
/// Float32 MPE tables may contain values outside the normalized range.
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
        if entries != Some(values.len()) || values.iter().any(|value| !value.is_finite()) {
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
}

/// One-dimensional curves used in ICC-style transfer stages.
///
/// ICC gamma and sampled curves operate on the encoded `[0, 1]` domain.
/// The signed sRGB variants are Theoria's extended-range built-ins, not an
/// assertion that an ICC TRC has that out-of-range behavior.
#[derive(Clone, Debug, PartialEq)]
pub struct TransferCurve(CurveKind);

/// A borrowed description of a validated transfer curve.
#[derive(Clone, Copy, Debug)]
pub enum TransferCurveKind<'a> {
    Identity,
    Gamma(f64),
    Sampled(&'a [f64]),
    IccParametric {
        function: u16,
        parameters: &'a [f64],
    },
    Mpe(&'a [MpeSegment]),
    SrgbDecode,
    SrgbEncode,
}

#[derive(Clone, Debug, PartialEq)]
enum CurveKind {
    Identity,
    Gamma(f64),
    Sampled(Arc<[f64]>),
    IccParametric {
        function: u16,
        parameters: Arc<[f64]>,
    },
    Mpe(Arc<[MpeSegment]>),
    SrgbDecode,
    SrgbEncode,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MpeSegment {
    pub(super) upper: f64,
    pub(super) kind: MpeSegmentKind,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) enum MpeSegmentKind {
    Formula { function: u16, parameters: Vec<f64> },
    Sampled { lower: f64, samples: Vec<f64> },
}

/// A borrowed description of a validated ICC multi-process curve segment.
#[derive(Clone, Copy, Debug)]
pub enum MpeSegmentView<'a> {
    Formula {
        function: u16,
        parameters: &'a [f64],
    },
    Sampled {
        lower: f64,
        samples: &'a [f64],
    },
}

impl MpeSegment {
    pub const fn upper(&self) -> f64 {
        self.upper
    }

    pub fn view(&self) -> MpeSegmentView<'_> {
        match &self.kind {
            MpeSegmentKind::Formula {
                function,
                parameters,
            } => MpeSegmentView::Formula {
                function: *function,
                parameters,
            },
            MpeSegmentKind::Sampled { lower, samples } => MpeSegmentView::Sampled {
                lower: *lower,
                samples,
            },
        }
    }
}

impl TransferCurve {
    pub const IDENTITY: Self = Self(CurveKind::Identity);
    pub const SRGB_DECODE: Self = Self(CurveKind::SrgbDecode);
    pub const SRGB_ENCODE: Self = Self(CurveKind::SrgbEncode);
    pub const SRGB_DECODE_THRESHOLD: f64 = SRGB_DECODE_THRESHOLD;
    pub const SRGB_ENCODE_THRESHOLD: f64 = SRGB_ENCODE_THRESHOLD;
    pub const SRGB_LINEAR_SCALE: f64 = SRGB_LINEAR_SCALE;
    pub const SRGB_POWER_SCALE: f64 = SRGB_POWER_SCALE;
    pub const SRGB_POWER_OFFSET: f64 = SRGB_POWER_OFFSET;
    pub const SRGB_GAMMA: f64 = SRGB_GAMMA;

    pub fn kind(&self) -> TransferCurveKind<'_> {
        match &self.0 {
            CurveKind::Identity => TransferCurveKind::Identity,
            CurveKind::Gamma(exponent) => TransferCurveKind::Gamma(*exponent),
            CurveKind::Sampled(values) => TransferCurveKind::Sampled(values),
            CurveKind::IccParametric {
                function,
                parameters,
            } => TransferCurveKind::IccParametric {
                function: *function,
                parameters,
            },
            CurveKind::Mpe(segments) => TransferCurveKind::Mpe(segments),
            CurveKind::SrgbDecode => TransferCurveKind::SrgbDecode,
            CurveKind::SrgbEncode => TransferCurveKind::SrgbEncode,
        }
    }

    pub(super) fn mpe(segments: Vec<MpeSegment>) -> Result<Self, TransferError> {
        if segments.is_empty()
            || segments.iter().any(|segment| match &segment.kind {
                MpeSegmentKind::Formula {
                    function,
                    parameters,
                } => {
                    let expected = match function {
                        0 => 4,
                        1 | 2 => 5,
                        _ => 0,
                    };
                    expected == 0
                        || parameters.len() != expected
                        || parameters.iter().any(|value| !value.is_finite())
                }
                MpeSegmentKind::Sampled { lower, samples } => {
                    !lower.is_finite()
                        || !segment.upper.is_finite()
                        || segment.upper <= *lower
                        || samples.len() < 2
                        || samples.iter().any(|value| !value.is_finite())
                }
            })
        {
            return Err(TransferError::InvalidCurve);
        }
        Ok(Self(CurveKind::Mpe(segments.into())))
    }

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
}

/// ICC D50 PCS white, expressed as the specified XYZ tristimulus values.
pub(super) const ICC_PCS_D50_XYZ: [f64; 3] = [0.9642, 1.0, 0.8249];
// Shared by CPU evaluation and shader generation of the sRGB transfer curve.
const SRGB_DECODE_THRESHOLD: f64 = 0.04045;
const SRGB_ENCODE_THRESHOLD: f64 = 0.003_130_8;
const SRGB_LINEAR_SCALE: f64 = 12.92;
const SRGB_POWER_SCALE: f64 = 1.055;
const SRGB_POWER_OFFSET: f64 = 0.055;
const SRGB_GAMMA: f64 = 2.4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransferError {
    EmptyChannels,
    ChannelMismatch { expected: usize, actual: usize },
    InvalidMatrixShape,
    InvalidClutShape,
    InvalidCurve,
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
            Self::NonFiniteValue => formatter.write_str("transfer data and values must be finite"),
        }
    }
}

impl Error for TransferError {}
