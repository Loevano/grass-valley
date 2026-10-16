//! Python JSON clock scalars, kept separate from binary64 gameplay quantities.
use super::json::Node;
use crate::BoundaryError;

#[derive(Clone)]
pub(super) enum Clock {
    Integer(String),
    Float(f64),
}
impl Clock {
    pub fn parse(node: &Node, path: &str) -> Result<Self, BoundaryError> {
        let Node::Scalar(raw) = node else {
            return Err(invalid(path));
        };
        let text = raw.get();
        if text.contains(['.', 'e', 'E']) {
            let value = text.parse::<f64>().map_err(|_| invalid(path))?;
            if value.is_finite() && value >= 0.0 {
                return Ok(Self::Float(value));
            }
        } else if text == "-0" || text.bytes().all(|b| b.is_ascii_digit()) {
            return Ok(Self::Integer(if text == "-0" { "0" } else { text }.into()));
        }
        Err(invalid(path))
    }
    pub fn zero() -> Self {
        Self::Integer("0".into())
    }
    pub fn equal(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Integer(a), Self::Integer(b)) => a == b,
            (Self::Float(a), Self::Float(b)) => a == b,
            (Self::Integer(integer), Self::Float(float))
            | (Self::Float(float), Self::Integer(integer)) => {
                // Python compares the exact integer with the float's exact
                // integral value, not with a rounded conversion of the integer.
                float.fract() == 0.0 && *integer == format!("{:.0}", float.abs())
            }
        }
    }
    pub fn binary64(&self, path: &str) -> Result<f64, BoundaryError> {
        let value = match self {
            Self::Float(v) => *v,
            Self::Integer(v) => v.parse().map_err(|_| overflow(path))?,
        };
        if value.is_finite() {
            Ok(value)
        } else {
            Err(overflow(path))
        }
    }
    /// max(0, now - previous): int subtraction happens BEFORE any float conversion.
    pub fn elapsed(&self, previous: &Self, path: &str) -> Result<Self, BoundaryError> {
        match (self, previous) {
            (Self::Integer(a), Self::Integer(b)) => {
                if (a.len(), a) <= (b.len(), b) {
                    return Ok(Self::zero());
                }
                let mut digits = Vec::with_capacity(a.len());
                let mut right = b.bytes().rev();
                let mut borrow = 0i16;
                for left in a.bytes().rev() {
                    let digit = i16::from(left - b'0')
                        - right.next().map_or(0, |v| i16::from(v - b'0'))
                        - borrow;
                    borrow = i16::from(digit < 0);
                    digits.push((digit.rem_euclid(10) as u8) + b'0');
                }
                while digits.len() > 1 && digits.last() == Some(&b'0') {
                    digits.pop();
                }
                digits.reverse();
                Ok(Self::Integer(String::from_utf8(digits).unwrap()))
            }
            _ => Ok(Self::Float(
                (self.binary64(path)? - previous.binary64(path)?).max(0.0),
            )),
        }
    }
    /// Python int/int true division rounds the exact quotient once; converting
    /// the numerator first can double-round (including at 2^53 + 1).
    pub fn thirds(&self, path: &str) -> Result<f64, BoundaryError> {
        let Self::Integer(integer) = self else {
            return Ok(self.binary64(path)? / 3.0);
        };
        let mut quotient = String::with_capacity(integer.len() + 81);
        let mut remainder = 0;
        for digit in integer.bytes() {
            let value = remainder * 10 + digit - b'0';
            let digit = value / 3;
            if !quotient.is_empty() || digit != 0 {
                quotient.push(char::from(b'0' + digit));
            }
            remainder = value % 3;
        }
        if quotient.is_empty() {
            quotient.push('0');
        }
        if remainder != 0 {
            quotient.push('.');
            // Positive int/3 is >= 1/3. Every binary64 rounding boundary in
            // this range has denominator <= 2^55. Eighty decimal fractional
            // digits therefore distinguish the repeating third from ANY tie.
            quotient.extend(std::iter::repeat_n(
                if remainder == 1 { '3' } else { '6' },
                80,
            ));
        }
        Self::Integer(quotient).binary64(path)
    }
}
fn invalid(path: &str) -> BoundaryError {
    BoundaryError::invalid(path, "expected finite nonnegative clock")
}
fn overflow(path: &str) -> BoundaryError {
    BoundaryError::invalid(path, "clock arithmetic overflow")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    #[derive(Deserialize)]
    struct ResultValue {
        bits: Option<u64>,
        integer: Option<String>,
        error: Option<String>,
    }
    #[derive(Deserialize)]
    struct Case {
        now: String,
        previous: String,
        equal: bool,
        elapsed: ResultValue,
        thirds: ResultValue,
        binary64: ResultValue,
    }
    fn parse(text: &str) -> Clock {
        Clock::parse(&Node::parse(text.as_bytes(), "$.clock").unwrap(), "$.clock").unwrap()
    }
    fn check(actual: Result<f64, BoundaryError>, expected: &ResultValue, label: &str) {
        if expected.error.is_some() {
            assert!(actual.is_err(), "{label}");
        } else {
            assert_eq!(actual.unwrap().to_bits(), expected.bits.unwrap(), "{label}");
        }
    }
    #[test]
    fn independent_python_clock_arithmetic() {
        let cases: Vec<Case> =
            serde_json::from_slice(include_bytes!("../../tests/fixtures/terrain/clocks.json"))
                .unwrap();
        assert_eq!(cases.len(), 3096);
        for case in cases {
            let a = parse(&case.now);
            let b = parse(&case.previous);
            let label = format!("{} - {}", case.now, case.previous);
            assert_eq!(a.equal(&b), case.equal, "{label}");
            let elapsed = a.elapsed(&b, "$.clock");
            if case.elapsed.error.is_some() {
                assert!(elapsed.is_err(), "{label}");
                continue;
            }
            let elapsed = elapsed.unwrap();
            match &elapsed {
                Clock::Integer(v) => assert_eq!(Some(v), case.elapsed.integer.as_ref(), "{label}"),
                Clock::Float(v) => {
                    // Python max(0, difference) keeps the first integer zero on ties.
                    if v == &0.0 {
                        assert_eq!(case.elapsed.integer.as_deref(), Some("0"), "{label}");
                    } else {
                        assert_eq!(Some(v.to_bits()), case.elapsed.bits, "{label}");
                    }
                }
            }
            check(elapsed.thirds("$.clock"), &case.thirds, &label);
            check(elapsed.binary64("$.clock"), &case.binary64, &label);
        }
    }
}
