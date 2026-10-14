//! Decimal literals rounded directly to IEEE 754 binary32, roundTiesToEven.
//!
//! Compare against exact decimal midpoints by bisecting positive float encodings.
//! This favors an auditable const implementation over fast conversion. No input
//! digits are truncated and no intermediate floating-point rounding is used.

use std::cmp::Ordering;

const INFINITY: u32 = 0x7f80_0000;
const SIGN: u32 = 0x8000_0000;
const SYNTAX: &str = "expected a signed decimal number (no exponent)";

pub(super) const fn parse(text: &str) -> Result<f32, &'static str> {
    let input = match Decimal::parse(text) {
        Ok(input) => input,
        Err(error) => return Err(error),
    };
    if input.first == input.end {
        return Ok(f32::from_bits(input.sign));
    }
    let mut low = 0;
    let mut high = INFINITY;
    while low < high {
        let lower = low + (high - low) / 2;
        let boundary = Boundary::above(lower);
        match input.compare(&boundary) {
            Ordering::Greater => low = lower + 1,
            Ordering::Equal if lower & 1 != 0 => low = lower + 1,
            _ => high = lower,
        }
    }
    if low == INFINITY {
        Err("decimal number rounds outside the finite f32 range")
    } else {
        Ok(f32::from_bits(low | input.sign))
    }
}

struct Decimal<'a> {
    bytes: &'a [u8],
    first: usize,
    last: usize,
    end: usize,
    point: isize,
    sign: u32,
}

impl<'a> Decimal<'a> {
    const fn parse(text: &'a str) -> Result<Self, &'static str> {
        let bytes = text.as_bytes();
        let mut start = 0;
        let mut end = bytes.len();
        // XML Schema decimal's lexical syntax and surrounding XML whitespace.
        // Its arbitrary-precision decimal value space is NOT our value space.
        while start < end && whitespace(bytes[start]) {
            start += 1;
        }
        while end > start && whitespace(bytes[end - 1]) {
            end -= 1;
        }
        let mut sign = 0;
        if start < end && (bytes[start] == b'+' || bytes[start] == b'-') {
            if bytes[start] == b'-' {
                sign = SIGN;
            }
            start += 1;
        }
        let mut first = end;
        let mut last = end;
        let mut digits = 0usize;
        let mut integer_digits = 0usize;
        let mut first_digit = 0usize;
        let mut dot = false;
        let mut i = start;
        while i < end {
            let byte = bytes[i];
            if byte >= b'0' && byte <= b'9' {
                if byte != b'0' {
                    if first == end {
                        first = i;
                        first_digit = digits;
                    }
                    last = i;
                }
                digits += 1;
                if !dot {
                    integer_digits += 1;
                }
            } else if byte == b'.' && !dot {
                dot = true;
            } else {
                return Err(SYNTAX);
            }
            i += 1;
        }
        if digits == 0 {
            return Err(SYNTAX);
        }
        Ok(Self {
            bytes,
            first,
            last,
            end,
            point: integer_digits as isize - first_digit as isize,
            sign,
        })
    }

    const fn compare(&self, boundary: &Boundary) -> Ordering {
        let point = boundary.len as isize + boundary.scale as isize;
        if self.point < point {
            return Ordering::Less;
        }
        if self.point > point {
            return Ordering::Greater;
        }
        let mut cursor = self.first;
        let mut i = boundary.len;
        while i > 0 {
            i -= 1;
            if cursor < self.end && self.bytes[cursor] == b'.' {
                cursor += 1;
            }
            let digit = if cursor < self.end {
                let digit = self.bytes[cursor] - b'0';
                cursor += 1;
                digit
            } else {
                0
            };
            if digit < boundary.digits[i] {
                return Ordering::Less;
            }
            if digit > boundary.digits[i] {
                return Ordering::Greater;
            }
        }
        // All remaining boundary digits are zero. Even a very late nonzero
        // input digit breaks a tie; do not round/truncate the input prefix.
        if self.last >= cursor {
            Ordering::Greater
        } else {
            Ordering::Equal
        }
    }
}

const fn whitespace(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\r' | b'\n')
}

struct Boundary {
    // Midpoints are m * 2^e, m < 2^25 and -150 <= e <= 103.
    // For e < 0, m * 5^(-e) < 10^113, with decimal scale e.
    // For e >= 0, m * 2^e < 2^128, requiring at most 39 digits.
    digits: [u8; 113],
    len: usize,
    scale: i32,
}

impl Boundary {
    const fn above(bits: u32) -> Self {
        let exponent = (bits >> 23) as i32;
        let fraction = bits & 0x7f_ffff;
        let (mut coefficient, power) = if exponent == 0 {
            (2 * fraction + 1, -150)
        } else {
            (2 * (0x80_0000 + fraction) + 1, exponent - 151)
        };
        // Above max finite, use the midpoint to the hypothetical value 2^128.
        // Ties there round to infinity, which parse reports as overflow.
        let mut value = Self {
            digits: [0; 113],
            len: 0,
            scale: if power < 0 { power } else { 0 },
        };
        while coefficient != 0 {
            value.digits[value.len] = (coefficient % 10) as u8;
            value.len += 1;
            coefficient /= 10;
        }
        let mut remaining = power.unsigned_abs();
        // Largest powers fitting u32; batch exact integer multiplication to
        // keep Rust const evaluation small. The digit/carry arithmetic uses u64.
        let (base, chunk) = if power < 0 { (5u32, 13) } else { (2u32, 31) };
        while remaining > 0 {
            let count = if remaining < chunk { remaining } else { chunk };
            value.multiply(base.pow(count));
            remaining -= count;
        }
        value
    }

    const fn multiply(&mut self, factor: u32) {
        let mut carry = 0u64;
        let mut i = 0;
        while i < self.len {
            let digit = self.digits[i] as u64 * factor as u64 + carry;
            self.digits[i] = (digit % 10) as u8;
            carry = digit / 10;
            i += 1;
        }
        while carry != 0 {
            self.digits[self.len] = (carry % 10) as u8;
            self.len += 1;
            carry /= 10;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::parse;

    const DECIMAL: f32 = match parse("0.3") {
        Ok(value) => value,
        Err(error) => panic!("{}", error),
    };
    const ABOVE_TIE: f32 = match parse("1.00000005960464477539062500001") {
        Ok(value) => value,
        Err(error) => panic!("{}", error),
    };

    fn agrees_with_standard(text: &str) {
        let expected = text.parse::<f32>().unwrap();
        if expected.is_infinite() {
            assert!(parse(text).is_err(), "{text}");
        } else {
            assert_eq!(parse(text).unwrap().to_bits(), expected.to_bits(), "{text}");
        }
    }

    #[test]
    fn const_conversion_and_lexical_rules() {
        assert_eq!(DECIMAL.to_bits(), 0x3e99_999a);
        assert_eq!(ABOVE_TIE.to_bits(), 0x3f80_0001);
        for text in ["0", "-0", "+0", ".5", "-.5", "+5.", "00012.34000"] {
            agrees_with_standard(text);
        }
        assert_eq!(parse(" \t+12.5\r\n").unwrap(), 12.5);
        for text in [
            "",
            " ",
            "+",
            "-",
            ".",
            "-.",
            "1..2",
            "1e2",
            "1E-2",
            "NaN",
            "inf",
            "-Infinity",
            "1 2",
            "1\t2",
            "１２",
            "\u{a0}1",
        ] {
            assert!(parse(text).is_err(), "{text:?}");
        }
    }

    #[test]
    fn rounds_ties_even_and_preserves_signed_underflow() {
        for (text, bits) in [
            ("1.000000059604644775390625", 0x3f80_0000),
            ("1.000000178813934326171875", 0x3f80_0002),
            ("16777217", 0x4b80_0000),
            ("16777219", 0x4b80_0002),
            (
                "-0.00000000000000000000000000000000000000000000001",
                0x8000_0000,
            ),
            ("0.000000000000000000000000000000000000000000001", 1),
            ("340282346638528859811704183484516925440", 0x7f7f_ffff),
            ("340282356779733661637539395458142568447", 0x7f7f_ffff),
        ] {
            assert_eq!(parse(text).unwrap().to_bits(), bits, "{text}");
            agrees_with_standard(text);
            agrees_with_standard(&format!("-{}", text.trim_start_matches('-')));
        }
        for text in [
            "340282356779733661637539395458142568448",
            "-340282356779733661637539395458142568448",
        ] {
            assert!(parse(text).is_err());
            agrees_with_standard(text);
        }
        let tie = "1.000000059604644775390625";
        assert_eq!(
            parse(&format!("{tie}{}1", "0".repeat(2000)))
                .unwrap()
                .to_bits(),
            0x3f80_0001
        );
        assert_eq!(
            parse(&format!("{tie}{}", "0".repeat(2000)))
                .unwrap()
                .to_bits(),
            0x3f80_0000
        );
        agrees_with_standard(&format!("{}1", "0".repeat(2000)));
        agrees_with_standard(&format!("0.{}1", "0".repeat(2000)));
        agrees_with_standard(&"9".repeat(2000));
    }

    // Independent test oracle for exact dyadic decimals: start from an integer,
    // divide the decimal digits by two repeatedly, appending a 5 on a remainder.
    fn dyadic(mut coefficient: u64, power: i32) -> String {
        if power >= 0 {
            return ((coefficient as u128) << power).to_string();
        }
        let mut digits = Vec::new();
        while coefficient > 0 {
            digits.push((coefficient % 10) as u8);
            coefficient /= 10;
        }
        digits.reverse();
        let point = digits.len();
        for _ in power..0 {
            let mut carry = 0;
            for digit in &mut digits {
                let value = carry * 10 + *digit;
                *digit = value / 2;
                carry = value % 2;
            }
            if carry != 0 {
                digits.push(5);
            }
        }
        let mut result = String::new();
        for (i, digit) in digits.iter().enumerate() {
            if i == point {
                result.push('.');
            }
            result.push(char::from(b'0' + digit));
        }
        result
    }

    #[test]
    fn every_exponent_boundary_and_neighboring_midpoints_match_standard() {
        for exponent in 0..255u32 {
            for fraction in [0, 1, 0x3f_ffff, 0x7f_fffe, 0x7f_ffff] {
                let bits = (exponent << 23) | fraction;
                let (coefficient, power) = if exponent == 0 {
                    (fraction as u64, -149)
                } else {
                    ((fraction | 0x80_0000) as u64, exponent as i32 - 150)
                };
                let mut midpoint = dyadic(2 * coefficient + 1, power - 1);
                if !midpoint.contains('.') {
                    midpoint.push('.');
                }
                // One exact decimal unit on either side, beyond all midpoint digits.
                let above = format!("{midpoint}0001");
                let mut below = format!("{midpoint}0000").into_bytes();
                for digit in below.iter_mut().rev() {
                    if *digit == b'.' {
                        continue;
                    }
                    if *digit != b'0' {
                        *digit -= 1;
                        break;
                    }
                    *digit = b'9';
                }
                let below = String::from_utf8(below).unwrap();
                assert_eq!(parse(&below).unwrap().to_bits(), bits);
                if bits + 1 < super::INFINITY {
                    assert_eq!(parse(&above).unwrap().to_bits(), bits + 1);
                    let expected = bits + (bits & 1);
                    assert_eq!(parse(&midpoint).unwrap().to_bits(), expected);
                } else {
                    assert!(parse(&above).is_err());
                    assert!(parse(&midpoint).is_err());
                }
                for text in [&below, &midpoint, &above] {
                    agrees_with_standard(text);
                    agrees_with_standard(&format!("-{text}"));
                }
            }
        }
    }

    #[test]
    fn varied_decimal_lengths_and_scales_match_standard() {
        let mut seed = 0xcafe_babe_1234_5678u64;
        for sample in 0..3000 {
            let len = 1 + sample % 173;
            let point = sample % (len + 1);
            let mut text = String::from(if sample % 2 == 0 { "-" } else { "+" });
            for i in 0..len {
                if i == point {
                    text.push('.');
                }
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                text.push(char::from(b'0' + (seed % 10) as u8));
            }
            agrees_with_standard(&text);
        }
    }
}
