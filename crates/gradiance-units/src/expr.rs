//! Typed-value entry: a small arithmetic parser with units (brief 5-3).
//!
//! `25 cm`, `90 deg`, `2 kN`, `2*pi`, `90/4`, `1 m/s^2` and `(1 + 2) cm`
//! all evaluate to a [`Value`]: a base-SI magnitude plus the exponents of
//! the base dimensions it carries ([`Dims`]). A field then fits the value to
//! its own [`Dimension`] ([`DisplayUnit::parse`](crate::DisplayUnit::parse)):
//! a bare number means the field's display unit, a value with units must
//! match the field's dimension.
//!
//! This is the **one** parser for typed values: number fields use it now,
//! and the number-field behaviours (brief U-8) and script units (brief 5-4)
//! are to reuse it rather than grow a second one. It is pure (no bevy, no
//! allocation beyond the error text), so the whole grammar is unit- and
//! property-tested here.
//!
//! # Grammar
//!
//! ```text
//! expr    := term (('+' | '-') term)*
//! term    := unary (('*' | '/' | '·' | '×' | '÷' | <juxtaposition>) unary)*
//! unary   := ('-' | '+' | '−') unary | power
//! power   := atom ('²' | '³')* ('^' unary)?
//! atom    := number | name | '(' expr ')'
//! ```
//!
//! Juxtaposition multiplies, so `25 cm` is `25 * cm` and `25 cm/s` is
//! `(25 * cm) / s`. A name is a unit (`m`, `cm`, `kN`, `deg`, `°`, `rpm`, …,
//! see [`UNITS`]) or a constant (`pi`, `tau`). Adding values of different
//! dimensions (`1 m + 1 s`) is an error, not a silent sum.

use crate::Dimension;
use thiserror::Error;

/// Exponents of the four base dimensions a value can carry: length (m),
/// mass (kg), time (s) and plane angle (rad).
///
/// Angle is kept as its own base so a frequency (`Hz`) cannot be typed into
/// an angular-velocity field and a length cannot be typed into an angle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct Dims {
    /// Length exponent (m).
    pub length: i8,
    /// Mass exponent (kg).
    pub mass: i8,
    /// Time exponent (s).
    pub time: i8,
    /// Plane-angle exponent (rad).
    pub angle: i8,
}

impl Dims {
    /// No dimension: a pure number.
    pub const NONE: Self = Self::new(0, 0, 0, 0);

    /// The exponents `m^length kg^mass s^time rad^angle`.
    #[must_use]
    pub const fn new(length: i8, mass: i8, time: i8, angle: i8) -> Self {
        Self {
            length,
            mass,
            time,
            angle,
        }
    }

    /// Whether this is a pure number.
    #[must_use]
    pub const fn is_none(self) -> bool {
        self.length == 0 && self.mass == 0 && self.time == 0 && self.angle == 0
    }

    fn combine(self, other: Self, sign: i8) -> Option<Self> {
        Some(Self {
            length: self.length.checked_add(other.length.checked_mul(sign)?)?,
            mass: self.mass.checked_add(other.mass.checked_mul(sign)?)?,
            time: self.time.checked_add(other.time.checked_mul(sign)?)?,
            angle: self.angle.checked_add(other.angle.checked_mul(sign)?)?,
        })
    }

    fn pow(self, n: i8) -> Option<Self> {
        Some(Self {
            length: self.length.checked_mul(n)?,
            mass: self.mass.checked_mul(n)?,
            time: self.time.checked_mul(n)?,
            angle: self.angle.checked_mul(n)?,
        })
    }

    /// The dimension these exponents name, if one does. Torque and energy
    /// share exponents (N·m = J); this answers the first in
    /// [`Dimension::ALL`] (torque), so compare exponents, not this, when
    /// fitting a field.
    #[must_use]
    pub fn dimension(self) -> Option<Dimension> {
        Dimension::ALL.into_iter().find(|d| d.dims() == self)
    }
}

impl core::fmt::Display for Dims {
    /// The exponents as unit symbols, e.g. `m s^-2` (`1` for none).
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if let Some(d) = self.dimension()
            && !d.symbol().is_empty()
        {
            return f.write_str(d.symbol());
        }
        if self.is_none() {
            return f.write_str("a plain number");
        }
        let mut first = true;
        for (sym, exp) in [
            ("m", self.length),
            ("kg", self.mass),
            ("s", self.time),
            ("rad", self.angle),
        ] {
            if exp == 0 {
                continue;
            }
            if !first {
                f.write_str(" ")?;
            }
            first = false;
            if exp == 1 {
                f.write_str(sym)?;
            } else {
                write!(f, "{sym}^{exp}")?;
            }
        }
        Ok(())
    }
}

/// An evaluated entry: a magnitude in base SI and the dimension it carries.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Value {
    /// The magnitude in base SI (`25 cm` is `0.25`).
    pub si: f64,
    /// The dimension's exponents.
    pub dims: Dims,
}

impl Value {
    const fn number(si: f64) -> Self {
        Self {
            si,
            dims: Dims::NONE,
        }
    }
}

/// Why a typed entry did not evaluate. The message is what a field or a
/// console shows.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ExprError {
    /// Nothing but whitespace.
    #[error("nothing to read")]
    Empty,
    /// A character the grammar does not use.
    #[error("unexpected `{0}`")]
    UnexpectedChar(char),
    /// A token in the wrong place (`2 * * 3`, `(1 + 2`).
    #[error("unexpected {0}")]
    Unexpected(String),
    /// A name that is neither a unit nor a constant.
    #[error("unknown unit `{0}`")]
    UnknownUnit(String),
    /// A sum of two different dimensions (`1 m + 1 s`).
    #[error("cannot add {0} and {1}")]
    Mismatch(String, String),
    /// A power of a value with units that is not a small whole number.
    #[error("a unit can only be raised to a whole power")]
    Power,
    /// The result is infinite or not a number (`1/0`).
    #[error("the result is not a finite number")]
    NotFinite,
    /// The value's dimension is not the field's (`25 cm` in an angle).
    #[error("expected {expected}, got {found}")]
    WrongDimension {
        /// The field's unit.
        expected: String,
        /// What the entry evaluated to.
        found: String,
    },
}

/// A named unit: its symbol, its size in base SI, and its dimension.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UnitDef {
    /// What is typed (`cm`, `kN`, `deg`).
    pub symbol: &'static str,
    /// One of it in base SI (`cm` is `0.01`).
    pub si: f64,
    /// Its dimension.
    pub dims: Dims,
}

const fn unit(symbol: &'static str, si: f64, dims: Dims) -> UnitDef {
    UnitDef { symbol, si, dims }
}

const LEN: Dims = Dims::new(1, 0, 0, 0);
const MASS: Dims = Dims::new(0, 1, 0, 0);
const TIME: Dims = Dims::new(0, 0, 1, 0);
const ANGLE: Dims = Dims::new(0, 0, 0, 1);
const FORCE: Dims = Dims::new(1, 1, -2, 0);
const ENERGY: Dims = Dims::new(2, 1, -2, 0);
const FREQ: Dims = Dims::new(0, 0, -1, 0);
const ANG_VEL: Dims = Dims::new(0, 0, -1, 1);

/// Every unit name the parser knows. SI prefixes are listed where a value
/// in the tool is plausibly typed with one; anything else composes
/// (`kg m/s`, `N*m`, `m^2`). Inch and foot are here because they cost
/// nothing; no other imperial unit is (brief 5-3 scope).
pub const UNITS: &[UnitDef] = &[
    // length
    unit("m", 1.0, LEN),
    unit("km", 1e3, LEN),
    unit("cm", 1e-2, LEN),
    unit("mm", 1e-3, LEN),
    unit("um", 1e-6, LEN),
    unit("in", 0.0254, LEN),
    unit("ft", 0.3048, LEN),
    // mass
    unit("kg", 1.0, MASS),
    unit("g", 1e-3, MASS),
    unit("mg", 1e-6, MASS),
    unit("t", 1e3, MASS),
    // time
    unit("s", 1.0, TIME),
    unit("ms", 1e-3, TIME),
    unit("min", 60.0, TIME),
    unit("h", 3600.0, TIME),
    // angle
    unit("rad", 1.0, ANGLE),
    unit("mrad", 1e-3, ANGLE),
    unit("deg", core::f64::consts::PI / 180.0, ANGLE),
    unit("\u{b0}", core::f64::consts::PI / 180.0, ANGLE),
    unit("rev", core::f64::consts::TAU, ANGLE),
    unit("turn", core::f64::consts::TAU, ANGLE),
    // derived
    unit("N", 1.0, FORCE),
    unit("mN", 1e-3, FORCE),
    unit("kN", 1e3, FORCE),
    unit("MN", 1e6, FORCE),
    unit("J", 1.0, ENERGY),
    unit("mJ", 1e-3, ENERGY),
    unit("kJ", 1e3, ENERGY),
    unit("Hz", 1.0, FREQ),
    unit("kHz", 1e3, FREQ),
    unit("rpm", core::f64::consts::TAU / 60.0, ANG_VEL),
];

/// The named constants (pure numbers).
pub const CONSTANTS: &[(&str, f64)] = &[
    ("pi", core::f64::consts::PI),
    ("tau", core::f64::consts::TAU),
];

/// The unit or constant called `name`.
fn lookup(name: &str) -> Option<Value> {
    if let Some(u) = UNITS.iter().find(|u| u.symbol == name) {
        return Some(Value {
            si: u.si,
            dims: u.dims,
        });
    }
    CONSTANTS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, v)| Value::number(*v))
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Token<'a> {
    Number(f64),
    Name(&'a str),
    Plus,
    Minus,
    Star,
    Slash,
    Caret,
    Open,
    Close,
    /// A superscript power (`²` = 2, `³` = 3).
    Sup(i8),
}

impl Token<'_> {
    fn describe(self) -> String {
        match self {
            Self::Number(n) => format!("number `{n}`"),
            Self::Name(n) => format!("`{n}`"),
            Self::Plus => "`+`".into(),
            Self::Minus => "`-`".into(),
            Self::Star => "`*`".into(),
            Self::Slash => "`/`".into(),
            Self::Caret => "`^`".into(),
            Self::Open => "`(`".into(),
            Self::Close => "`)`".into(),
            Self::Sup(n) => format!("superscript {n}"),
        }
    }
}

fn tokenize(text: &str) -> Result<Vec<Token<'_>>, ExprError> {
    let mut out = Vec::new();
    let mut chars = text.char_indices().peekable();
    while let Some(&(start, c)) = chars.peek() {
        let token = match c {
            c if c.is_whitespace() => {
                chars.next();
                continue;
            }
            '0'..='9' | '.' => {
                let end = number_end(text, start);
                while chars.peek().is_some_and(|&(i, _)| i < end) {
                    chars.next();
                }
                let n = text[start..end]
                    .parse::<f64>()
                    .map_err(|_| ExprError::Unexpected(format!("number `{}`", &text[start..end])))?;
                out.push(Token::Number(n));
                continue;
            }
            c if c.is_ascii_alphabetic() || c == '_' => {
                let mut end = start;
                while let Some(&(i, c)) = chars.peek() {
                    if c.is_ascii_alphanumeric() || c == '_' {
                        end = i + c.len_utf8();
                        chars.next();
                    } else {
                        break;
                    }
                }
                out.push(Token::Name(&text[start..end]));
                continue;
            }
            '\u{b0}' => Token::Name("\u{b0}"),
            '+' => Token::Plus,
            '-' | '\u{2212}' => Token::Minus,
            '*' | '\u{b7}' | '\u{d7}' => Token::Star,
            '/' | '\u{f7}' => Token::Slash,
            '^' => Token::Caret,
            '(' => Token::Open,
            ')' => Token::Close,
            '\u{b2}' => Token::Sup(2),
            '\u{b3}' => Token::Sup(3),
            other => return Err(ExprError::UnexpectedChar(other)),
        };
        chars.next();
        out.push(token);
    }
    Ok(out)
}

/// The byte index just past a number starting at `start`: digits with one
/// optional point, then an exponent only when digits follow it (`2e3`, but
/// `2e` is `2` then the name `e`).
fn number_end(text: &str, start: usize) -> usize {
    let bytes = text.as_bytes();
    let mut i = start;
    let mut seen_point = false;
    while i < bytes.len() && (bytes[i].is_ascii_digit() || (bytes[i] == b'.' && !seen_point)) {
        seen_point |= bytes[i] == b'.';
        i += 1;
    }
    if i < bytes.len() && (bytes[i] == b'e' || bytes[i] == b'E') {
        let mut j = i + 1;
        if j < bytes.len() && (bytes[j] == b'+' || bytes[j] == b'-') {
            j += 1;
        }
        if j < bytes.len() && bytes[j].is_ascii_digit() {
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                j += 1;
            }
            return j;
        }
    }
    i
}

struct Parser<'a> {
    tokens: Vec<Token<'a>>,
    at: usize,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<Token<'a>> {
        self.tokens.get(self.at).copied()
    }

    fn bump(&mut self) -> Option<Token<'a>> {
        let t = self.peek();
        self.at += 1;
        t
    }

    fn expr(&mut self) -> Result<Value, ExprError> {
        let mut acc = self.term()?;
        while let Some(op @ (Token::Plus | Token::Minus)) = self.peek() {
            self.bump();
            let rhs = self.term()?;
            if acc.dims != rhs.dims {
                return Err(ExprError::Mismatch(
                    acc.dims.to_string(),
                    rhs.dims.to_string(),
                ));
            }
            acc.si = if op == Token::Plus {
                acc.si + rhs.si
            } else {
                acc.si - rhs.si
            };
        }
        Ok(acc)
    }

    fn term(&mut self) -> Result<Value, ExprError> {
        let mut acc = self.unary()?;
        loop {
            let divide = match self.peek() {
                Some(Token::Star) => {
                    self.bump();
                    false
                }
                Some(Token::Slash) => {
                    self.bump();
                    true
                }
                // Juxtaposition: `25 cm`, `2 pi`, `3 (1 + 1)`.
                Some(Token::Number(_) | Token::Name(_) | Token::Open) => false,
                _ => return Ok(acc),
            };
            let rhs = self.unary()?;
            let (si, sign) = if divide {
                (acc.si / rhs.si, -1)
            } else {
                (acc.si * rhs.si, 1)
            };
            acc = Value {
                si,
                dims: acc.dims.combine(rhs.dims, sign).ok_or(ExprError::Power)?,
            };
        }
    }

    fn unary(&mut self) -> Result<Value, ExprError> {
        match self.peek() {
            Some(Token::Minus) => {
                self.bump();
                let v = self.unary()?;
                Ok(Value { si: -v.si, ..v })
            }
            Some(Token::Plus) => {
                self.bump();
                self.unary()
            }
            _ => self.power(),
        }
    }

    fn power(&mut self) -> Result<Value, ExprError> {
        let mut base = self.atom()?;
        while let Some(Token::Sup(n)) = self.peek() {
            self.bump();
            base = raise(base, f64::from(n))?;
        }
        if self.peek() == Some(Token::Caret) {
            self.bump();
            let exponent = self.unary()?;
            if !exponent.dims.is_none() {
                return Err(ExprError::Power);
            }
            base = raise(base, exponent.si)?;
        }
        Ok(base)
    }

    fn atom(&mut self) -> Result<Value, ExprError> {
        match self.bump() {
            Some(Token::Number(n)) => Ok(Value::number(n)),
            Some(Token::Name(name)) => {
                lookup(name).ok_or_else(|| ExprError::UnknownUnit(name.to_owned()))
            }
            Some(Token::Open) => {
                let v = self.expr()?;
                match self.bump() {
                    Some(Token::Close) => Ok(v),
                    Some(t) => Err(ExprError::Unexpected(t.describe())),
                    None => Err(ExprError::Unexpected("end: a `(` is not closed".into())),
                }
            }
            Some(t) => Err(ExprError::Unexpected(t.describe())),
            None => Err(ExprError::Unexpected("end".into())),
        }
    }
}

/// `base` to the power `exponent`. A value with units takes only a whole
/// power that keeps every exponent in range.
fn raise(base: Value, exponent: f64) -> Result<Value, ExprError> {
    if base.dims.is_none() {
        return Ok(Value::number(base.si.powf(exponent)));
    }
    if exponent.fract().abs() > 0.0 || exponent.abs() > 8.0 {
        return Err(ExprError::Power);
    }
    // Checked above: a whole number in [-8, 8].
    let n = exponent as i8;
    Ok(Value {
        si: base.si.powi(i32::from(n)),
        dims: base.dims.pow(n).ok_or(ExprError::Power)?,
    })
}

/// Evaluates a typed entry to a base-SI magnitude and its dimension.
///
/// ```
/// use gradiance_units::expr::{evaluate, Dims};
/// let v = evaluate("25 cm").unwrap();
/// assert!((v.si - 0.25).abs() < 1e-12);
/// assert_eq!(v.dims, Dims::new(1, 0, 0, 0));
/// assert!((evaluate("90/4").unwrap().si - 22.5).abs() < 1e-12);
/// ```
///
/// # Errors
/// [`ExprError`] for an empty entry, an unknown unit, a malformed
/// expression, a sum of different dimensions, or a non-finite result.
pub fn evaluate(text: &str) -> Result<Value, ExprError> {
    let tokens = tokenize(text)?;
    if tokens.is_empty() {
        return Err(ExprError::Empty);
    }
    let mut parser = Parser { tokens, at: 0 };
    let value = parser.expr()?;
    if let Some(t) = parser.peek() {
        return Err(ExprError::Unexpected(t.describe()));
    }
    if !value.si.is_finite() {
        return Err(ExprError::NotFinite);
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn si(text: &str) -> f64 {
        evaluate(text)
            .unwrap_or_else(|e| panic!("`{text}`: {e}"))
            .si
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() <= 1e-9 * b.abs().max(1.0)
    }

    #[test]
    fn numbers_and_arithmetic() {
        for (text, want) in [
            ("1.5", 1.5),
            ("1.5e3", 1500.0),
            ("2E-2", 0.02),
            (".5", 0.5),
            ("2*pi", core::f64::consts::TAU),
            ("90/4", 22.5),
            ("1 + 2 * 3", 7.0),
            ("(1 + 2) * 3", 9.0),
            ("-2^2", -4.0),
            ("2^-1", 0.5),
            ("2^3^2", 512.0),
            ("10 - 4 - 3", 3.0),
            ("2 pi", core::f64::consts::TAU),
            ("3 (1 + 1)", 6.0),
            ("1 \u{2212} 3", -2.0),
            ("6 \u{f7} 4 \u{d7} 2", 3.0),
            ("  7  ", 7.0),
        ] {
            assert!(close(si(text), want), "`{text}` = {} not {want}", si(text));
        }
    }

    #[test]
    fn every_unit_has_its_size_and_dimension() {
        for (text, want, dims) in [
            ("25 cm", 0.25, LEN),
            ("25cm", 0.25, LEN),
            ("3 mm", 0.003, LEN),
            ("1.2 km", 1200.0, LEN),
            ("1 in", 0.0254, LEN),
            ("90 deg", core::f64::consts::FRAC_PI_2, ANGLE),
            ("90\u{b0}", core::f64::consts::FRAC_PI_2, ANGLE),
            ("1 rev", core::f64::consts::TAU, ANGLE),
            ("2 kN", 2000.0, FORCE),
            ("500 g", 0.5, MASS),
            ("2 min", 120.0, TIME),
            ("60 rpm", core::f64::consts::TAU, ANG_VEL),
            ("3 kHz", 3000.0, FREQ),
            ("25 cm/s", 0.25, Dimension::Velocity.dims()),
            ("9.81 m/s^2", 9.81, Dimension::Acceleration.dims()),
            ("9.81 m/s\u{b2}", 9.81, Dimension::Acceleration.dims()),
            ("2 kg/m\u{b2}", 2.0, Dimension::Density.dims()),
            ("3 N\u{b7}m", 3.0, Dimension::Torque.dims()),
            ("4 N*s/m", 4.0, Dimension::Damping.dims()),
            ("1 kg m\u{b2}/s", 1.0, Dimension::AngularMomentum.dims()),
            ("90 deg/s", core::f64::consts::FRAC_PI_2, ANG_VEL),
            ("1 m + 50 cm", 1.5, LEN),
            ("(1 + 2) cm", 0.03, LEN),
            ("1/2 m", 0.5, LEN),
            ("pi rad", core::f64::consts::PI, ANGLE),
        ] {
            let v = evaluate(text).unwrap_or_else(|e| panic!("`{text}`: {e}"));
            assert!(close(v.si, want), "`{text}` = {} not {want}", v.si);
            assert_eq!(v.dims, dims, "`{text}`");
        }
    }

    #[test]
    fn every_unit_name_evaluates_and_is_unique() {
        for (i, u) in UNITS.iter().enumerate() {
            let v = evaluate(&format!("1 {}", u.symbol)).expect(u.symbol);
            assert_eq!(v.dims, u.dims, "{}", u.symbol);
            assert!(
                UNITS[i + 1..].iter().all(|o| o.symbol != u.symbol),
                "{} twice",
                u.symbol
            );
            assert!(CONSTANTS.iter().all(|(n, _)| *n != u.symbol));
        }
    }

    #[test]
    fn every_dimension_symbol_reads_back_as_its_dimension() {
        // What a field shows (`2.5 kg/m²`) must be what it accepts.
        for d in Dimension::ALL {
            let v = evaluate(&format!("2.5 {}", d.symbol()))
                .unwrap_or_else(|e| panic!("{d:?} `{}`: {e}", d.symbol()));
            assert_eq!(v.dims, d.dims(), "{d:?}");
            assert!(close(v.si, 2.5), "{d:?}");
        }
    }

    #[test]
    fn errors_say_what_went_wrong() {
        assert_eq!(evaluate("   "), Err(ExprError::Empty));
        assert_eq!(evaluate("2 $"), Err(ExprError::UnexpectedChar('$')));
        assert_eq!(
            evaluate("3 furlongs"),
            Err(ExprError::UnknownUnit("furlongs".into()))
        );
        assert!(matches!(evaluate("1 m + 1 s"), Err(ExprError::Mismatch(..))));
        assert!(matches!(evaluate("2 * * 3"), Err(ExprError::Unexpected(_))));
        assert!(matches!(evaluate("(1 + 2"), Err(ExprError::Unexpected(_))));
        assert!(matches!(evaluate("1 + 2)"), Err(ExprError::Unexpected(_))));
        assert!(matches!(evaluate("1 +"), Err(ExprError::Unexpected(_))));
        assert_eq!(evaluate("1/0"), Err(ExprError::NotFinite));
        assert_eq!(evaluate("m^0.5"), Err(ExprError::Power));
        assert_eq!(evaluate("2^m"), Err(ExprError::Power));
        assert_eq!(evaluate("m^100"), Err(ExprError::Power));
    }

    #[test]
    fn a_dimension_prints_as_its_symbol() {
        assert_eq!(Dims::new(1, 0, -1, 0).to_string(), "m/s");
        assert_eq!(Dims::NONE.to_string(), "a plain number");
        assert_eq!(Dims::new(3, 0, 0, 0).to_string(), "m^3");
        assert_eq!(Dims::new(1, 0, 0, 1).to_string(), "m rad");
    }

    mod props {
        use super::super::*;
        use proptest::prelude::*;

        proptest! {
            /// Anything typed either evaluates or is an error: never a
            /// panic, never a non-finite value.
            #[test]
            fn arbitrary_text_never_panics(text in "\\PC{0,24}") {
                if let Ok(v) = evaluate(&text) {
                    prop_assert!(v.si.is_finite());
                }
            }

            /// Text built from the grammar's own pieces never panics either
            /// (the random strings above rarely reach deep paths).
            #[test]
            fn grammar_soup_never_panics(
                parts in proptest::collection::vec(
                    proptest::sample::select(vec![
                        "1", "2.5", "1e3", "pi", "m", "cm", "s", "deg", "kN", "(", ")",
                        "+", "-", "*", "/", "^", "\u{b2}", " ", "rpm", "0",
                    ]),
                    0..16,
                )
            ) {
                let text: String = parts.concat();
                if let Ok(v) = evaluate(&text) {
                    prop_assert!(v.si.is_finite());
                }
            }

            /// A finite number printed with any unit reads back as that
            /// number times the unit's size.
            #[test]
            fn number_with_unit_round_trips(
                n in -1e6_f64..1e6,
                u in 0..UNITS.len(),
            ) {
                let unit = UNITS[u];
                let v = evaluate(&format!("{n} {}", unit.symbol)).unwrap();
                prop_assert_eq!(v.dims, unit.dims);
                let want = n * unit.si;
                prop_assert!((v.si - want).abs() <= 1e-9 * want.abs().max(1.0));
            }

            /// Addition and multiplication agree with f64 on plain numbers.
            #[test]
            fn arithmetic_matches_f64(a in -1e3_f64..1e3, b in -1e3_f64..1e3) {
                let sum = evaluate(&format!("{a} + {b}")).unwrap().si;
                prop_assert!((sum - (a + b)).abs() <= 1e-9 * (a + b).abs().max(1.0));
                let product = evaluate(&format!("({a}) * ({b})")).unwrap().si;
                prop_assert!((product - a * b).abs() <= 1e-9 * (a * b).abs().max(1.0));
            }
        }
    }
}
