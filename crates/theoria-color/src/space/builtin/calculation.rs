//! Private expression helpers for declaring builtin calculator programs.
//! Only stack instructions survive construction; renderers need no knowledge
//! of these helpers or of the color conversion being described.
use crate::transfer::{
    CalculatorBinary as B, CalculatorOperation as O, CalculatorUnary as U, Transfer, TransferError,
};

#[derive(Clone)]
pub(super) struct Expression(Vec<O>);

pub(super) fn input(index: usize) -> Expression {
    Expression(vec![O::Input(index)])
}
pub(super) fn number(value: f64) -> Expression {
    Expression(vec![O::Constant(value)])
}

pub(super) fn transfer(outputs: [Expression; 3]) -> Result<Transfer, TransferError> {
    Transfer::calculator(
        3,
        3,
        outputs
            .into_iter()
            .flat_map(|value| value.0)
            .collect::<Vec<_>>(),
    )
}

impl Expression {
    pub(super) fn unary(mut self, operation: U) -> Self {
        self.0.push(O::Unary(operation));
        self
    }
    pub(super) fn binary(mut self, other: Self, operation: B) -> Self {
        self.0.extend(other.0);
        self.0.push(O::Binary(operation));
        self
    }
    pub(super) fn choose(mut self, when_true: Self, when_false: Self) -> Self {
        self.0.push(O::IfElse {
            when_true: when_true.0.into(),
            when_false: when_false.0.into(),
        });
        self
    }
    pub(super) fn min(self, other: Self) -> Self {
        self.binary(other, B::Min)
    }
    pub(super) fn max(self, other: Self) -> Self {
        self.binary(other, B::Max)
    }
    pub(super) fn eq(self, other: Self) -> Self {
        self.binary(other, B::Equal)
    }
    pub(super) fn lt(self, other: Self) -> Self {
        self.binary(other, B::Less)
    }
    pub(super) fn ge(self, other: Self) -> Self {
        self.binary(other, B::GreaterEqual)
    }
    pub(super) fn modulo(self, period: f64) -> Self {
        // Positive-period Euclidean remainder, preserving multi-turn inputs.
        let remainder = self.binary(number(period), B::Remainder);
        remainder
            .clone()
            .lt(number(0.0))
            .choose(remainder.clone() + number(period), remainder)
    }
}

macro_rules! arithmetic {
    ($trait:ident, $method:ident, $operation:ident) => {
        impl std::ops::$trait for Expression {
            type Output = Self;
            fn $method(self, other: Self) -> Self {
                self.binary(other, B::$operation)
            }
        }
    };
}
arithmetic!(Add, add, Add);
arithmetic!(Sub, sub, Subtract);
arithmetic!(Mul, mul, Multiply);
arithmetic!(Div, div, Divide);
