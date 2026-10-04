use super::TransferError;

/// A validated, scalar stack program. Channels are independent of color models.
///
/// This is a backend-neutral calculation vocabulary, not an iccMAX binary
/// decoder. Scalar operations can express vector operations by repetition.
/// The final stack, from bottom to top, is the output channel sequence.
#[derive(Clone, Debug, PartialEq)]
pub struct TransferCalculator {
    pub(super) input_channels: usize,
    pub(super) output_channels: usize,
    operations: Box<[CalculatorOperation]>,
}

/// Binary operations pop the right operand, then the left operand.
/// Comparisons produce 0 or 1. Angles are in radians.
#[derive(Clone, Debug, PartialEq)]
pub enum CalculatorOperation {
    Input(usize),
    Constant(f64),
    /// Copies a value at this depth below the top (zero copies the top).
    Duplicate(usize),
    Discard,
    Unary(CalculatorUnary),
    Binary(CalculatorBinary),
    /// Pops a condition; evaluates only the selected branch. Values >= 0.5
    /// select `when_true`, following the iccMAX numeric boolean convention.
    IfElse {
        when_true: Box<[Self]>,
        when_false: Box<[Self]>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CalculatorUnary {
    Negate,
    Abs,
    SquareRoot,
    CubeRoot,
    Floor,
    Ceil,
    Truncate,
    Round,
    Exp,
    Ln,
    Log10,
    Sin,
    Cos,
    Tan,
    Asin,
    Acos,
    Atan,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CalculatorBinary {
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
    Power,
    Min,
    Max,
    /// atan2(right, left), matching iccMAX's X, Y operand order.
    Atan2,
    Equal,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
}

impl TransferCalculator {
    pub(super) fn new(
        input_channels: usize,
        output_channels: usize,
        operations: Box<[CalculatorOperation]>,
    ) -> Result<Self, TransferError> {
        if input_channels == 0 || output_channels == 0 {
            return Err(TransferError::EmptyChannels);
        }
        let count = validate(&operations, input_channels, 0, 0)?;
        if count != output_channels {
            return Err(TransferError::ChannelMismatch {
                expected: output_channels,
                actual: count,
            });
        }
        Ok(Self {
            input_channels,
            output_channels,
            operations,
        })
    }

    pub fn input_channels(&self) -> usize {
        self.input_channels
    }
    pub fn output_channels(&self) -> usize {
        self.output_channels
    }
    pub fn operations(&self) -> &[CalculatorOperation] {
        &self.operations
    }

    pub(super) fn evaluate(&self, input: &[f64]) -> Vec<f64> {
        let mut stack = Vec::new();
        evaluate(&self.operations, input, &mut stack);
        stack
    }
}

fn validate(
    operations: &[CalculatorOperation],
    inputs: usize,
    mut stack: usize,
    depth: usize,
) -> Result<usize, TransferError> {
    // Bound recursive validation, evaluation and shader generation together.
    if depth > 64 {
        return Err(TransferError::InvalidCalculator);
    }
    for operation in operations {
        match operation {
            CalculatorOperation::Input(index) => {
                if *index >= inputs {
                    return Err(TransferError::InvalidCalculator);
                }
                stack += 1;
            }
            CalculatorOperation::Constant(value) => {
                if !value.is_finite() {
                    return Err(TransferError::NonFiniteValue);
                }
                stack += 1;
            }
            CalculatorOperation::Duplicate(index) => {
                if *index >= stack {
                    return Err(TransferError::InvalidCalculator);
                }
                stack += 1;
            }
            CalculatorOperation::Discard => {
                stack = stack
                    .checked_sub(1)
                    .ok_or(TransferError::InvalidCalculator)?;
            }
            CalculatorOperation::Unary(_) => {
                if stack == 0 {
                    return Err(TransferError::InvalidCalculator);
                }
            }
            CalculatorOperation::Binary(_) => {
                if stack < 2 {
                    return Err(TransferError::InvalidCalculator);
                }
                stack -= 1;
            }
            CalculatorOperation::IfElse {
                when_true,
                when_false,
            } => {
                stack = stack
                    .checked_sub(1)
                    .ok_or(TransferError::InvalidCalculator)?;
                let a = validate(when_true, inputs, stack, depth + 1)?;
                let b = validate(when_false, inputs, stack, depth + 1)?;
                if a != b {
                    return Err(TransferError::InvalidCalculator);
                }
                stack = a;
            }
        }
    }
    Ok(stack)
}

fn evaluate(operations: &[CalculatorOperation], input: &[f64], stack: &mut Vec<f64>) {
    for operation in operations {
        match operation {
            CalculatorOperation::Input(index) => stack.push(input[*index]),
            CalculatorOperation::Constant(value) => stack.push(*value),
            CalculatorOperation::Duplicate(depth) => stack.push(stack[stack.len() - 1 - depth]),
            CalculatorOperation::Discard => {
                stack.pop();
            }
            CalculatorOperation::Unary(operation) => {
                let value = stack.pop().expect("validated calculator");
                stack.push(operation.evaluate(value));
            }
            CalculatorOperation::Binary(operation) => {
                let right = stack.pop().expect("validated calculator");
                let left = stack.pop().expect("validated calculator");
                stack.push(operation.evaluate(left, right));
            }
            CalculatorOperation::IfElse {
                when_true,
                when_false,
            } => {
                let condition = stack.pop().expect("validated calculator");
                evaluate(
                    if condition >= 0.5 {
                        when_true
                    } else {
                        when_false
                    },
                    input,
                    stack,
                );
            }
        }
    }
}

impl CalculatorUnary {
    fn evaluate(self, value: f64) -> f64 {
        match self {
            Self::Negate => -value,
            Self::Abs => value.abs(),
            Self::SquareRoot => value.sqrt(),
            Self::CubeRoot => value.cbrt(),
            Self::Floor => value.floor(),
            Self::Ceil => value.ceil(),
            Self::Truncate => value.trunc(),
            Self::Round => value.round(),
            Self::Exp => value.exp(),
            Self::Ln => value.ln(),
            Self::Log10 => value.log10(),
            Self::Sin => value.sin(),
            Self::Cos => value.cos(),
            Self::Tan => value.tan(),
            Self::Asin => value.asin(),
            Self::Acos => value.acos(),
            Self::Atan => value.atan(),
        }
    }
}

impl CalculatorBinary {
    fn evaluate(self, left: f64, right: f64) -> f64 {
        match self {
            Self::Add => left + right,
            Self::Subtract => left - right,
            Self::Multiply => left * right,
            Self::Divide => left / right,
            Self::Remainder => left % right,
            Self::Power => left.powf(right),
            Self::Min => left.min(right),
            Self::Max => left.max(right),
            Self::Atan2 => right.atan2(left),
            Self::Equal => f64::from(left == right),
            Self::Less => f64::from(left < right),
            Self::LessEqual => f64::from(left <= right),
            Self::Greater => f64::from(left > right),
            Self::GreaterEqual => f64::from(left >= right),
        }
    }
}
