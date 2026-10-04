use theoria_color::{CalculatorBinary, CalculatorOperation, CalculatorUnary, TransferCalculator};

use super::{TransferWgslError, wgsl_literal};

pub(super) fn wgsl(
    calculator: &TransferCalculator,
    name: &str,
) -> Result<String, TransferWgslError> {
    let mut body = String::new();
    let mut stack = Vec::new();
    emit(calculator.operations(), &mut stack, &mut body, &mut 0)?;
    Ok(format!(
        "fn {name}(value: array<f32, {}>) -> array<f32, {}> {{\n{body}return array<f32, {}>({});\n}}\n",
        calculator.input_channels(),
        calculator.output_channels(),
        calculator.output_channels(),
        stack.join(", ")
    ))
}

fn emit(
    operations: &[CalculatorOperation],
    stack: &mut Vec<String>,
    body: &mut String,
    next: &mut usize,
) -> Result<(), TransferWgslError> {
    for operation in operations {
        let expression = match operation {
            CalculatorOperation::Input(index) => format!("value[{index}]"),
            CalculatorOperation::Constant(value) => wgsl_literal(*value)?,
            CalculatorOperation::Duplicate(depth) => stack[stack.len() - 1 - depth].clone(),
            CalculatorOperation::Discard => {
                stack.pop();
                continue;
            }
            CalculatorOperation::Unary(operation) => {
                unary(*operation, &stack.pop().expect("validated calculator"))
            }
            CalculatorOperation::Binary(operation) => {
                let right = stack.pop().expect("validated calculator");
                let left = stack.pop().expect("validated calculator");
                binary(*operation, &left, &right)
            }
            CalculatorOperation::IfElse {
                when_true,
                when_false,
            } => {
                let condition = stack.pop().expect("validated calculator");
                let mut true_stack = stack.clone();
                let mut false_stack = stack.clone();
                let mut true_body = String::new();
                let mut false_body = String::new();
                emit(when_true, &mut true_stack, &mut true_body, next)?;
                emit(when_false, &mut false_stack, &mut false_body, next)?;
                stack.clear();
                for (a, b) in true_stack.iter().zip(&false_stack) {
                    let result = variable(next);
                    body.push_str(&format!("var {result}: f32;\n"));
                    true_body.push_str(&format!("{result} = {a};\n"));
                    false_body.push_str(&format!("{result} = {b};\n"));
                    stack.push(result);
                }
                body.push_str(&format!(
                    "if {condition} >= 0.5 {{\n{true_body}}} else {{\n{false_body}}}\n"
                ));
                continue;
            }
        };
        let result = variable(next);
        body.push_str(&format!("let {result} = {expression};\n"));
        stack.push(result);
    }
    Ok(())
}

fn variable(next: &mut usize) -> String {
    let result = format!("calc_{}", *next);
    *next += 1;
    result
}

fn unary(operation: CalculatorUnary, value: &str) -> String {
    let function = match operation {
        CalculatorUnary::Negate => return format!("-{value}"),
        CalculatorUnary::CubeRoot => {
            return format!("sign({value}) * pow(abs({value}), 1.0 / 3.0)");
        }
        // WGSL round uses ties-to-even; the calculator uses ties-away-from-zero.
        CalculatorUnary::Round => return format!("sign({value}) * floor(abs({value}) + 0.5)"),
        CalculatorUnary::Log10 => return format!("log({value}) / log(10.0)"),
        CalculatorUnary::Abs => "abs",
        CalculatorUnary::SquareRoot => "sqrt",
        CalculatorUnary::Floor => "floor",
        CalculatorUnary::Ceil => "ceil",
        CalculatorUnary::Truncate => "trunc",
        CalculatorUnary::Exp => "exp",
        CalculatorUnary::Ln => "log",
        CalculatorUnary::Sin => "sin",
        CalculatorUnary::Cos => "cos",
        CalculatorUnary::Tan => "tan",
        CalculatorUnary::Asin => "asin",
        CalculatorUnary::Acos => "acos",
        CalculatorUnary::Atan => "atan",
    };
    format!("{function}({value})")
}

fn binary(operation: CalculatorBinary, left: &str, right: &str) -> String {
    let operator = match operation {
        CalculatorBinary::Add => "+",
        CalculatorBinary::Subtract => "-",
        CalculatorBinary::Multiply => "*",
        CalculatorBinary::Divide => "/",
        CalculatorBinary::Remainder => {
            return format!("{left} - trunc({left} / {right}) * {right}");
        }
        CalculatorBinary::Power => return format!("pow({left}, {right})"),
        CalculatorBinary::Min => return format!("min({left}, {right})"),
        CalculatorBinary::Max => return format!("max({left}, {right})"),
        CalculatorBinary::Atan2 => return format!("atan2({right}, {left})"),
        CalculatorBinary::Equal => "==",
        CalculatorBinary::Less => "<",
        CalculatorBinary::LessEqual => "<=",
        CalculatorBinary::Greater => ">",
        CalculatorBinary::GreaterEqual => ">=",
    };
    match operation {
        CalculatorBinary::Equal
        | CalculatorBinary::Less
        | CalculatorBinary::LessEqual
        | CalculatorBinary::Greater
        | CalculatorBinary::GreaterEqual => format!("select(0.0, 1.0, {left} {operator} {right})"),
        _ => format!("{left} {operator} {right}"),
    }
}
