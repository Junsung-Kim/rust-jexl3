// port of: java.math.BigDecimal, java.math.MathContext, java.math.RoundingMode
//! `java.math.BigDecimal` (JDK 25) on top of `num_bigint::BigInt`.
//!
//! The JDK keeps a `long` fast path next to the `BigInteger` one; both compute the same values, so
//! this port only keeps the general algorithm, except where the two paths throw different
//! exceptions (see [`BigDecimal::divide_scale`]). Methods that can throw in Java return
//! `Result<_, MathError>`; the infallible convenience methods (`add`, `subtract`, `multiply`,
//! `strip_trailing_zeros`, `to_big_integer`) have `try_*` twins that report Java's exception.
use crate::java::number::{big_integer_hash_code, char_digit, double_to_string};
use crate::java::string::{JString, JStringBuilder};
use num_bigint::{BigInt, BigUint, Sign};
use num_integer::Integer;
use num_traits::{One, Signed, ToPrimitive, Zero};
use std::cmp::Ordering;
use std::fmt;

/// `java.math.RoundingMode`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RoundingMode {
    Up,
    Down,
    Ceiling,
    Floor,
    HalfUp,
    HalfDown,
    HalfEven,
    Unnecessary,
}

impl RoundingMode {
    const ALL: [RoundingMode; 8] = [
        Self::Up,
        Self::Down,
        Self::Ceiling,
        Self::Floor,
        Self::HalfUp,
        Self::HalfDown,
        Self::HalfEven,
        Self::Unnecessary,
    ];

    /// `Enum.name()`.
    pub fn name(self) -> &'static str {
        match self {
            Self::Up => "UP",
            Self::Down => "DOWN",
            Self::Ceiling => "CEILING",
            Self::Floor => "FLOOR",
            Self::HalfUp => "HALF_UP",
            Self::HalfDown => "HALF_DOWN",
            Self::HalfEven => "HALF_EVEN",
            Self::Unnecessary => "UNNECESSARY",
        }
    }

    /// `RoundingMode.valueOf(String)`; `None` where Java throws IllegalArgumentException.
    pub fn value_of(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.name() == s)
    }
}

impl fmt::Display for RoundingMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// `java.math.MathContext`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MathContext {
    pub precision: u32,
    pub rounding_mode: RoundingMode,
}

impl MathContext {
    pub const UNLIMITED: MathContext = MathContext::new(0, RoundingMode::HalfUp);
    pub const DECIMAL32: MathContext = MathContext::new(7, RoundingMode::HalfEven);
    pub const DECIMAL64: MathContext = MathContext::new(16, RoundingMode::HalfEven);
    pub const DECIMAL128: MathContext = MathContext::new(34, RoundingMode::HalfEven);

    pub const fn new(precision: u32, rounding_mode: RoundingMode) -> Self {
        MathContext { precision, rounding_mode }
    }

    fn prec(&self) -> i64 {
        self.precision as i64
    }
}

impl fmt::Display for MathContext {
    /// `MathContext.toString()`: `precision=34 roundingMode=HALF_EVEN`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "precision={} roundingMode={}", self.precision, self.rounding_mode)
    }
}

/// Java `ArithmeticException` / `NumberFormatException` raised by BigDecimal operations. The
/// payload is `getMessage()`; a `null` message (e.g. `new BigDecimal("")`) is the empty string.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MathError {
    Arithmetic(JString),
    NumberFormat(JString),
}

type R = Result<BigDecimal, MathError>;

fn arith<T>(m: &str) -> Result<T, MathError> {
    let m: String = m.into();
    Err(MathError::Arithmetic(JString::from(m)))
}

fn number_format<T>(m: impl Into<String>) -> Result<T, MathError> {
    let m: String = m.into();
    Err(MathError::NumberFormat(JString::from(m)))
}

/// `java.math.BigDecimal`: `unscaled × 10^-scale`.
#[derive(Clone, Debug)]
pub struct BigDecimal {
    int_val: BigInt,
    scale: i32,
}

// ------------------------------------------------------------------------------------------
// BigInteger helpers mirroring the JDK's limits.

/// `BigDecimal.bigTenToThe(n)` for n >= 0. Java's BigInteger cannot hold 2^31 bits or more:
/// 10^n with n >= 646456993 throws "BigInteger would overflow supported range".
fn ten_pow(n: i64) -> Result<BigInt, MathError> {
    if n >= 646_456_993 {
        return arith("BigInteger would overflow supported range");
    }
    let n = n.max(0) as u32;
    Ok(if n < 19 { BigInt::from(10u64.pow(n)) } else { BigInt::from(10u32).pow(n) })
}

/// `BigInteger.pow(int)`: Java throws once the result would reach 2^31 bits.
fn big_pow(b: &BigInt, n: u32) -> Result<BigInt, MathError> {
    if n == 0 {
        return Ok(BigInt::one());
    }
    if b.is_zero() {
        return Ok(b.clone());
    }
    let overflow = || arith("BigInteger would overflow supported range");
    let tz = b.magnitude().trailing_zeros().unwrap_or(0);
    let odd = b.magnitude() >> tz;
    let shift = tz * n as u64;
    if (odd.bits() - 1) * n as u64 + 1 + shift > i32::MAX as u64 {
        return overflow();
    }
    let p = odd.pow(n);
    if p.bits() + shift > i32::MAX as u64 {
        return overflow();
    }
    let sign = if b.sign() == Sign::Minus && n % 2 == 1 { Sign::Minus } else { Sign::Plus };
    Ok(BigInt::from_biguint(sign, p << shift))
}

/// Number of decimal digits (1 for zero): `longDigitLength` / `bigDigitLength`.
fn digit_length(b: &BigInt) -> i64 {
    let m = b.magnitude();
    if let Some(v) = m.to_u64() {
        return if v == 0 { 1 } else { v.ilog10() as i64 + 1 };
    }
    let r = ((m.bits() + 1) * 646_456_993) >> 31;
    if *m < BigUint::from(10u32).pow(r as u32) {
        r as i64
    } else {
        r as i64 + 1
    }
}

/// Java's `intCompact != INFLATED`: the unscaled value fits a long other than Long.MIN_VALUE.
fn is_compact(b: &BigInt) -> bool {
    b.bits() < 64
}

fn saturate(s: i64) -> i32 {
    s.clamp(i32::MIN as i64, i32::MAX as i64) as i32
}

/// `checkScale(long)`: out-of-range scales throw unless the value is zero (then they saturate).
fn check_scale(nonzero: bool, val: i64) -> Result<i32, MathError> {
    match i32::try_from(val) {
        Ok(v) => Ok(v),
        Err(_) if nonzero => arith(if val > i32::MAX as i64 { "Underflow" } else { "Overflow" }),
        Err(_) => Ok(saturate(val)),
    }
}

/// `checkScaleNonZero(long)`: unlike `checkScale`, the message follows the sign of `(int) val`.
fn check_scale_nz(val: i64) -> Result<i32, MathError> {
    match i32::try_from(val) {
        Ok(v) => Ok(v),
        Err(_) => arith(if val as i32 > 0 { "Underflow" } else { "Overflow" }),
    }
}

/// `commonNeedIncrement`: whether a truncated quotient must be bumped away from zero.
fn need_increment(rm: RoundingMode, qsign: i32, cmp_frac_half: Ordering, odd: bool) -> Result<bool, MathError> {
    Ok(match rm {
        RoundingMode::Unnecessary => return arith("Rounding necessary"),
        RoundingMode::Up => true,
        RoundingMode::Down => false,
        RoundingMode::Ceiling => qsign > 0,
        RoundingMode::Floor => qsign < 0,
        _ => match cmp_frac_half {
            Ordering::Less => false,
            Ordering::Greater => true,
            Ordering::Equal => match rm {
                RoundingMode::HalfDown => false,
                RoundingMode::HalfUp => true,
                _ => odd,
            },
        },
    })
}

/// `divideAndRound(dividend, divisor, roundingMode)`; also reports whether the division was exact.
fn div_round(n: &BigInt, d: &BigInt, rm: RoundingMode) -> Result<(BigInt, bool), MathError> {
    let (q, r) = n.div_rem(d);
    if r.is_zero() {
        return Ok((q, true));
    }
    let qsign = if n.sign() == d.sign() { 1 } else { -1 };
    let cmp = (r.magnitude() << 1u32).cmp(d.magnitude());
    let odd = q.is_odd();
    Ok(if need_increment(rm, qsign, cmp, odd)? { (q + qsign, false) } else { (q, false) })
}

impl BigDecimal {
    // --------------------------------------------------------------------------------------
    // Construction

    /// `new BigDecimal(BigInteger unscaledVal, int scale)`.
    pub fn new(unscaled: BigInt, scale: i32) -> Self {
        BigDecimal { int_val: unscaled, scale }
    }

    fn of(int_val: BigInt, scale: i32) -> R {
        Ok(Self::new(int_val, scale))
    }

    pub fn zero() -> Self {
        Self::from_i64(0)
    }

    pub fn one() -> Self {
        Self::from_i64(1)
    }

    pub fn ten() -> Self {
        Self::from_i64(10)
    }

    /// `BigDecimal.valueOf(long)` / `new BigDecimal(long)`.
    pub fn from_i64(v: i64) -> Self {
        Self::new(BigInt::from(v), 0)
    }

    /// `new BigDecimal(BigInteger)`.
    pub fn from_bigint(b: &BigInt) -> Self {
        Self::new(b.clone(), 0)
    }

    /// `BigDecimal.valueOf(double)`: the digits of `Double.toString(d)`.
    pub fn value_of_double(d: f64) -> R {
        if !d.is_finite() {
            return number_format("Infinite or NaN");
        }
        Self::parse(&double_to_string(d))
    }

    /// `new BigDecimal(double)`: the exact binary value.
    pub fn from_double_exact(d: f64) -> R {
        if !d.is_finite() {
            return number_format("Infinite or NaN");
        }
        let bits = d.to_bits();
        let exponent = ((bits >> 52) & 0x7ff) as i32;
        let fraction = bits & ((1 << 52) - 1);
        let mut significand = if exponent == 0 { fraction << 1 } else { fraction | (1 << 52) };
        if significand == 0 {
            return Ok(Self::zero());
        }
        let mut exponent = exponent - 1075;
        let tz = significand.trailing_zeros();
        significand >>= tz;
        exponent += tz as i32;
        let mut v = BigInt::from(significand);
        if d < 0.0 {
            v = -v;
        }
        Ok(match exponent.cmp(&0) {
            Ordering::Equal => Self::new(v, 0),
            Ordering::Less => Self::new(v * BigInt::from(5u32).pow(-exponent as u32), -exponent),
            Ordering::Greater => Self::new(v << exponent as usize, 0),
        })
    }

    /// `new BigDecimal(String)`.
    /// new BigDecimal(String) over UTF-16 units (the Java String the caller really has)
    pub fn parse_units(u: &[u16]) -> R {
        Self::parse_units_with(u, &MathContext::UNLIMITED)
    }

    pub fn parse(s: &str) -> R {
        Self::parse_with(s, &MathContext::UNLIMITED)
    }

    /// `new BigDecimal(String, MathContext)` over a Rust string.
    pub fn parse_with(s: &str, mc: &MathContext) -> R {
        let u: Vec<u16> = s.encode_utf16().collect();
        Self::parse_units_with(&u, mc)
    }

    /// `new BigDecimal(String, MathContext)` (`BigDecimal(char[], int, int, MathContext)`).
    pub fn parse_units_with(u: &[u16], mc: &MathContext) -> R {
        let first = *u.first().ok_or_else(|| MathError::NumberFormat(JString::empty()))?;
        let mut i = 0;
        let isneg = first == b'-' as u16;
        if isneg || first == b'+' as u16 {
            i = 1;
        }
        // The JDK scans inputs of at most 18 chars (after the sign) with a different loop that
        // reports a different message for a bad character.
        let compact = u.len() - i <= 18;
        let mut seen_digit = false;
        let mut sig: Vec<u8> = Vec::new(); // digits from the first non-zero one
        let mut scl: i64 = 0;
        let mut dot = false;
        while i < u.len() {
            let c = u[i];
            if let Some(d) = decimal_digit(c) {
                seen_digit = true;
                if d != 0 || !sig.is_empty() {
                    sig.push(d);
                }
                if dot {
                    scl += 1;
                }
            } else if c == b'.' as u16 {
                if dot {
                    return number_format("Character array contains more than one decimal point.");
                }
                dot = true;
            } else if c == b'e' as u16 || c == b'E' as u16 {
                scl -= parse_exp(u, i)?;
                break;
            } else if compact {
                return Err(MathError::NumberFormat(
                    JStringBuilder::new()
                        .str("Character ")
                        .units(&[c])
                        .str(" is neither a decimal digit number, decimal point, nor \"e\" notation exponential mark.")
                        .build(),
                ));
            } else {
                return number_format("Character array is missing \"e\" notation exponential mark.");
            }
            i += 1;
        }
        if !seen_digit {
            return number_format("No digits found.");
        }
        let mut prec = sig.len().max(1) as i64;
        let mut v = if sig.is_empty() {
            BigInt::zero()
        } else {
            BigInt::from_radix_be(if isneg { Sign::Minus } else { Sign::Plus }, &sig, 10).unwrap_or_default()
        };
        let mcp = mc.prec();
        while mcp > 0 && prec > mcp {
            let drop = prec - mcp;
            scl -= drop;
            v = div_round(&v, &ten_pow(drop)?, mc.rounding_mode)?.0;
            prec = digit_length(&v);
        }
        match i32::try_from(scl) {
            Ok(scale) => Self::of(v, scale),
            Err(_) => number_format("Exponent overflow."),
        }
    }

    // --------------------------------------------------------------------------------------
    // Accessors

    /// `unscaledValue()`.
    pub fn unscaled_value(&self) -> &BigInt {
        &self.int_val
    }

    pub fn scale(&self) -> i32 {
        self.scale
    }

    /// Number of digits of the unscaled value (1 for zero).
    pub fn precision(&self) -> u32 {
        digit_length(&self.int_val) as u32
    }

    fn prec(&self) -> i64 {
        digit_length(&self.int_val)
    }

    pub fn signum(&self) -> i32 {
        match self.int_val.sign() {
            Sign::Minus => -1,
            Sign::NoSign => 0,
            Sign::Plus => 1,
        }
    }

    fn is_zero(&self) -> bool {
        self.int_val.is_zero()
    }

    /// `this.checkScale(long)`.
    fn check_scale(&self, val: i64) -> Result<i32, MathError> {
        check_scale(!self.is_zero(), val)
    }

    // --------------------------------------------------------------------------------------
    // Addition, subtraction, multiplication

    /// `add(BigDecimal)`.
    pub fn try_add(&self, o: &Self) -> R {
        let sdiff = self.scale as i64 - o.scale as i64;
        let (lo, hi) = match sdiff.cmp(&0) {
            Ordering::Equal => return Self::of(&self.int_val + &o.int_val, self.scale),
            Ordering::Less => (self, o),
            Ordering::Greater => (o, self),
        };
        // `lo` has the smaller scale and gets raised; a zero is never multiplied out.
        if lo.is_zero() {
            return Self::of(hi.int_val.clone(), hi.scale);
        }
        let raise = lo.check_scale(sdiff.abs())?;
        Self::of(&lo.int_val * ten_pow(raise as i64)? + &hi.int_val, hi.scale)
    }

    /// `add(BigDecimal)`. Java throws where the scales are too far apart to align; this returns
    /// `self` unchanged then (use [`Self::try_add`] to mirror the exception).
    pub fn add(&self, o: &Self) -> Self {
        self.try_add(o).unwrap_or_else(|_| self.clone())
    }

    /// `add(BigDecimal, MathContext)`.
    pub fn add_mc(&self, o: &Self, mc: &MathContext) -> R {
        if mc.precision == 0 {
            return self.try_add(o);
        }
        if self.is_zero() || o.is_zero() {
            let preferred = self.scale.max(o.scale);
            if self.is_zero() && o.is_zero() {
                return Self::of(BigInt::zero(), preferred);
            }
            let result = if self.is_zero() { o.round(mc)? } else { self.round(mc)? };
            return match result.scale.cmp(&preferred) {
                Ordering::Equal => Ok(result),
                Ordering::Greater => strip_zeros(result.int_val, result.scale, preferred as i64),
                Ordering::Less => {
                    let precision_diff = (mc.precision as i32).wrapping_sub(result.precision() as i32);
                    let scale_diff = preferred.wrapping_sub(result.scale);
                    if precision_diff >= scale_diff {
                        result.set_scale(preferred, RoundingMode::Unnecessary)
                    } else {
                        result.set_scale(result.scale.wrapping_add(precision_diff), RoundingMode::Unnecessary)
                    }
                }
            };
        }
        let padding = self.scale as i64 - o.scale as i64;
        let (mut lhs, mut augend) = (self.clone(), o.clone());
        if padding != 0 {
            // preAlign: an operand far below the result's ulp only matters as a sticky digit.
            let (big, mut small) = if padding < 0 { (self, o.clone()) } else { (o, self.clone()) };
            let est_ulp_scale = big.scale as i64 - big.prec() + mc.prec();
            let small_high = small.scale as i64 - small.prec() + 1;
            if small_high > big.scale as i64 + 2 && small_high > est_ulp_scale + 2 {
                let scale = self.check_scale((big.scale as i64).max(est_ulp_scale) + 3)?;
                small = Self::new(BigInt::from(small.signum()), scale);
            }
            // matchScale
            lhs = big.clone();
            augend = small;
            if lhs.scale < augend.scale {
                lhs = lhs.set_scale(augend.scale, RoundingMode::Unnecessary)?;
            } else if augend.scale < lhs.scale {
                augend = augend.set_scale(lhs.scale, RoundingMode::Unnecessary)?;
            }
        }
        Self::new(lhs.int_val + augend.int_val, lhs.scale).round(mc)
    }

    /// `subtract(BigDecimal)`.
    pub fn try_subtract(&self, o: &Self) -> R {
        self.try_add(&o.negate())
    }

    /// `subtract(BigDecimal)`; see [`Self::add`] for the Java-throws case.
    pub fn subtract(&self, o: &Self) -> Self {
        self.try_subtract(o).unwrap_or_else(|_| self.clone())
    }

    /// `subtract(BigDecimal, MathContext)`.
    pub fn subtract_mc(&self, o: &Self, mc: &MathContext) -> R {
        if mc.precision == 0 {
            return self.try_subtract(o);
        }
        self.add_mc(&o.negate(), mc)
    }

    /// `multiply(BigDecimal)`.
    pub fn try_multiply(&self, o: &Self) -> R {
        let scale = self.check_scale(self.scale as i64 + o.scale as i64)?;
        Self::of(&self.int_val * &o.int_val, scale)
    }

    /// `multiply(BigDecimal)`. Java throws when the product's scale overflows an int; this
    /// returns `self` unchanged then (use [`Self::try_multiply`] to mirror the exception).
    pub fn multiply(&self, o: &Self) -> Self {
        self.try_multiply(o).unwrap_or_else(|_| self.clone())
    }

    /// `multiply(BigDecimal, MathContext)`.
    pub fn multiply_mc(&self, o: &Self, mc: &MathContext) -> R {
        self.try_multiply(o)?.round(mc)
    }

    // --------------------------------------------------------------------------------------
    // Division

    /// `divide(BigDecimal, MathContext)`.
    pub fn divide_mc(&self, o: &Self, mc: &MathContext) -> R {
        let mcp = mc.prec();
        if mcp == 0 {
            return self.divide(o);
        }
        let preferred = self.scale as i64 - o.scale as i64;
        if o.is_zero() {
            return arith(if self.is_zero() { "Division undefined" } else { "Division by zero" });
        }
        if self.is_zero() {
            return Self::of(BigInt::zero(), saturate(preferred));
        }
        let xscale = self.prec();
        let mut yscale = o.prec();
        // compareMagnitudeNormalized: compare the digit strings as if both were 0.ddd
        let (xs, ys) = (self.int_val.magnitude(), o.int_val.magnitude());
        let sdiff = xscale - yscale;
        let cmp = if sdiff < 0 {
            (xs * ten_pow(-sdiff)?.magnitude()).cmp(ys)
        } else {
            xs.cmp(&(ys * ten_pow(sdiff)?.magnitude()))
        };
        if cmp == Ordering::Greater {
            yscale -= 1;
        }
        let scl = check_scale_nz(preferred + yscale - xscale + mcp)?;
        let raise = check_scale_nz(mcp + yscale - xscale)? as i64;
        let (n, d) = if raise > 0 {
            (&self.int_val * ten_pow(raise)?, o.int_val.clone())
        } else {
            let new_scale = check_scale_nz(xscale - mcp)? as i64;
            let raise = check_scale_nz(new_scale - yscale)? as i64;
            (self.int_val.clone(), &o.int_val * ten_pow(raise)?)
        };
        let preferred = check_scale_nz(preferred)?;
        divide_and_round(&n, &d, scl, mc.rounding_mode, preferred)?.round(mc)
    }

    /// `divide(BigDecimal)`: the exact quotient, or "Non-terminating decimal expansion".
    pub fn divide(&self, o: &Self) -> R {
        if o.is_zero() {
            return arith(if self.is_zero() { "Division undefined" } else { "Division by zero" });
        }
        let preferred = saturate(self.scale as i64 - o.scale as i64);
        if self.is_zero() {
            return Self::of(BigInt::zero(), preferred);
        }
        let prec = (self.prec() + (10 * o.prec() + 2) / 3).min(i32::MAX as i64) as u32;
        let mc = MathContext::new(prec, RoundingMode::Unnecessary);
        let quotient = self
            .divide_mc(o, &mc)
            .or_else(|_| arith("Non-terminating decimal expansion; no exact representable decimal result."))?;
        if preferred > quotient.scale {
            return quotient.set_scale(preferred, RoundingMode::Unnecessary);
        }
        Ok(quotient)
    }

    /// `divide(BigDecimal, int scale, RoundingMode)`.
    ///
    /// Division by zero surfaces as the JDK's raw `long` or `BigInteger` division error, so this
    /// follows the JDK's choice between its `long` and `BigInteger` code paths.
    pub fn divide_scale(&self, o: &Self, scale: i32, rm: RoundingMode) -> R {
        let (dividend_scale, divisor_scale) = (self.scale, o.scale);
        let both_compact = is_compact(&self.int_val) && is_compact(&o.int_val);
        // longMultiplyPowerTen(v, raise) != INFLATED
        let fits_long =
            |v: &BigInt, raise: i32| raise < 19 && (raise <= 0 || (v * ten_pow(raise as i64).unwrap_or_default()).bits() < 64);
        let (n, d, long_path);
        let cond = check_scale(!self.is_zero(), scale as i64 + divisor_scale as i64)?;
        if cond > dividend_scale {
            let raise = scale.wrapping_add(divisor_scale).wrapping_sub(dividend_scale);
            long_path = both_compact && fits_long(&self.int_val, raise);
            n = if raise > 0 { &self.int_val * ten_pow(raise as i64)? } else { self.int_val.clone() };
            d = o.int_val.clone();
        } else {
            let new_scale = check_scale(!o.is_zero(), dividend_scale as i64 - scale as i64)?;
            let raise = new_scale.wrapping_sub(divisor_scale);
            long_path = both_compact && fits_long(&o.int_val, raise);
            n = self.int_val.clone();
            d = if raise > 0 { &o.int_val * ten_pow(raise as i64)? } else { o.int_val.clone() };
        }
        if d.is_zero() {
            return arith(if long_path { "/ by zero" } else { "BigInteger divide by zero" });
        }
        divide_and_round(&n, &d, scale, rm, scale)
    }

    /// `divideToIntegralValue(BigDecimal, MathContext)`; precision 0 means the unlimited
    /// `divideToIntegralValue(BigDecimal)`.
    pub fn divide_to_integral_value(&self, o: &Self, mc: &MathContext) -> R {
        if mc.precision == 0 || self.compare_magnitude(o) == Ordering::Less {
            return self.divide_to_integral_value_exact(o);
        }
        let preferred = saturate(self.scale as i64 - o.scale as i64);
        let mut result = self.divide_mc(o, &MathContext::new(mc.precision, RoundingMode::Down))?;
        if result.scale < 0 {
            let product = result.try_multiply(o)?;
            if self.try_subtract(&product)?.compare_magnitude(o) != Ordering::Less {
                return arith("Division impossible");
            }
        } else if result.scale > 0 {
            result = result.set_scale(0, RoundingMode::Down)?;
        }
        let precision_diff = (mc.precision as i32).wrapping_sub(result.precision() as i32);
        if preferred > result.scale && precision_diff > 0 {
            let s = result.scale.wrapping_add(precision_diff.min(preferred.wrapping_sub(result.scale)));
            result.set_scale(s, RoundingMode::Unnecessary)
        } else {
            strip_zeros(result.int_val, result.scale, preferred as i64)
        }
    }

    /// `divideToIntegralValue(BigDecimal)`.
    fn divide_to_integral_value_exact(&self, o: &Self) -> R {
        let preferred = saturate(self.scale as i64 - o.scale as i64);
        if self.compare_magnitude(o) == Ordering::Less {
            return Self::of(BigInt::zero(), preferred);
        }
        if self.is_zero() && !o.is_zero() {
            return self.set_scale(preferred, RoundingMode::Unnecessary);
        }
        let max_digits = (self.prec() + (10 * o.prec() + 2) / 3 + (self.scale as i64 - o.scale as i64).abs() + 2)
            .min(i32::MAX as i64) as u32;
        let mut quotient = self.divide_mc(o, &MathContext::new(max_digits, RoundingMode::Down))?;
        if quotient.scale > 0 {
            quotient = quotient.set_scale(0, RoundingMode::Down)?;
            quotient = strip_zeros(quotient.int_val, quotient.scale, preferred as i64)?;
        }
        if quotient.scale < preferred {
            quotient = quotient.set_scale(preferred, RoundingMode::Unnecessary)?;
        }
        Ok(quotient)
    }

    /// `remainder(BigDecimal, MathContext)`.
    pub fn remainder_mc(&self, o: &Self, mc: &MathContext) -> R {
        let q = self.divide_to_integral_value(o, mc)?;
        self.try_subtract(&q.try_multiply(o)?)
    }

    /// `remainder(BigDecimal)`.
    pub fn remainder(&self, o: &Self) -> R {
        self.remainder_mc(o, &MathContext::UNLIMITED)
    }

    // --------------------------------------------------------------------------------------
    // Unary operations

    pub fn negate(&self) -> Self {
        Self::new(-&self.int_val, self.scale)
    }

    pub fn abs(&self) -> Self {
        Self::new(self.int_val.abs(), self.scale)
    }

    /// `pow(int)`.
    pub fn pow(&self, n: i32) -> R {
        if !(0..=999_999_999).contains(&n) {
            return arith("Invalid operation");
        }
        let scale = self.check_scale(self.scale as i64 * n as i64)?;
        Self::of(big_pow(&self.int_val, n as u32)?, scale)
    }

    /// `pow(int, MathContext)` (the X3.274 algorithm, rounding at every step).
    pub fn pow_mc(&self, n: i32, mc: &MathContext) -> R {
        if mc.precision == 0 {
            return self.pow(n);
        }
        if !(-999_999_999..=999_999_999).contains(&n) {
            return arith("Invalid operation");
        }
        if n == 0 {
            return Ok(Self::one());
        }
        let mut mag = n.unsigned_abs() as i32;
        let elength = digit_length(&BigInt::from(mag));
        if elength > mc.prec() {
            return arith("Invalid operation");
        }
        let workmc = MathContext::new(mc.precision.saturating_add(elength as u32 + 1), mc.rounding_mode);
        let mut acc = Self::one();
        let mut seenbit = false;
        for i in 1.. {
            mag = mag.wrapping_add(mag);
            if mag < 0 {
                seenbit = true;
                acc = acc.multiply_mc(self, &workmc)?;
            }
            if i == 31 {
                break;
            }
            if seenbit {
                acc = acc.multiply_mc(&acc, &workmc)?;
            }
        }
        if n < 0 {
            acc = Self::one().divide_mc(&acc, &workmc)?;
        }
        acc.round(mc)
    }

    /// `round(MathContext)` (`doRound`).
    pub fn round(&self, mc: &MathContext) -> R {
        let mcp = mc.prec();
        let mut v = self.int_val.clone();
        let mut scale = self.scale;
        if mcp > 0 {
            let mut prec = digit_length(&v);
            while prec > mcp {
                let drop = prec - mcp;
                scale = check_scale_nz(scale as i64 - drop)?;
                v = div_round(&v, &ten_pow(drop)?, mc.rounding_mode)?.0;
                prec = digit_length(&v);
            }
        }
        Self::of(v, scale)
    }

    /// `setScale(int, RoundingMode)`.
    pub fn set_scale(&self, new_scale: i32, rm: RoundingMode) -> R {
        let old = self.scale;
        if new_scale == old {
            return Ok(self.clone());
        }
        if self.is_zero() {
            return Self::of(BigInt::zero(), new_scale);
        }
        if new_scale > old {
            let raise = self.check_scale(new_scale as i64 - old as i64)?;
            Self::of(&self.int_val * ten_pow(raise as i64)?, new_scale)
        } else {
            let drop = self.check_scale(old as i64 - new_scale as i64)?;
            Self::of(div_round(&self.int_val, &ten_pow(drop as i64)?, rm)?.0, new_scale)
        }
    }

    /// `stripTrailingZeros()`.
    pub fn try_strip_trailing_zeros(&self) -> R {
        if self.is_zero() {
            return Ok(Self::zero());
        }
        strip_zeros(self.int_val.clone(), self.scale, i64::MIN)
    }

    /// `stripTrailingZeros()`. Java throws "Overflow" when stripping would push the scale below
    /// Integer.MIN_VALUE; this returns `self` unchanged then (see [`Self::try_strip_trailing_zeros`]).
    pub fn strip_trailing_zeros(&self) -> Self {
        self.try_strip_trailing_zeros().unwrap_or_else(|_| self.clone())
    }

    /// `movePointLeft(int)`.
    pub fn move_point_left(&self, n: i32) -> R {
        self.move_point(self.scale as i64 + n as i64, n == 0)
    }

    /// `movePointRight(int)`.
    pub fn move_point_right(&self, n: i32) -> R {
        self.move_point(self.scale as i64 - n as i64, n == 0)
    }

    fn move_point(&self, new_scale: i64, no_move: bool) -> R {
        if no_move && self.scale >= 0 {
            return Ok(self.clone());
        }
        let num = Self::new(self.int_val.clone(), self.check_scale(new_scale)?);
        if num.scale < 0 {
            num.set_scale(0, RoundingMode::Unnecessary)
        } else {
            Ok(num)
        }
    }

    /// `ulp()`.
    pub fn ulp(&self) -> Self {
        Self::new(BigInt::one(), self.scale)
    }

    pub fn max(&self, o: &Self) -> Self {
        if self.compare_to(o) != Ordering::Less { self.clone() } else { o.clone() }
    }

    pub fn min(&self, o: &Self) -> Self {
        if self.compare_to(o) != Ordering::Greater { self.clone() } else { o.clone() }
    }

    // --------------------------------------------------------------------------------------
    // Comparison, equality, hashing

    /// `compareTo(BigDecimal)`: numeric comparison ignoring scale.
    pub fn compare_to(&self, o: &Self) -> Ordering {
        let (xs, ys) = (self.signum(), o.signum());
        if xs != ys {
            return xs.cmp(&ys);
        }
        match xs {
            0 => Ordering::Equal,
            1 => self.compare_magnitude(o),
            _ => o.compare_magnitude(self),
        }
    }

    fn compare_magnitude(&self, o: &Self) -> Ordering {
        match (self.is_zero(), o.is_zero()) {
            (true, true) => return Ordering::Equal,
            (true, false) => return Ordering::Less,
            (false, true) => return Ordering::Greater,
            _ => {}
        }
        let (x, y) = (self.int_val.magnitude(), o.int_val.magnitude());
        let sdiff = self.scale as i64 - o.scale as i64;
        if sdiff == 0 {
            return x.cmp(y);
        }
        // Adjusted exponents first, so that no huge power of ten is ever built.
        let xae = self.prec() - self.scale as i64;
        let yae = o.prec() - o.scale as i64;
        if xae != yae {
            return xae.cmp(&yae);
        }
        let p = BigUint::from(10u32).pow(sdiff.unsigned_abs() as u32);
        if sdiff < 0 {
            (x * p).cmp(y)
        } else {
            x.cmp(&(y * p))
        }
    }

    /// `equals(Object)`: equal value and scale.
    pub fn java_equals(&self, o: &Self) -> bool {
        self.scale == o.scale && self.int_val == o.int_val
    }

    /// `hashCode()`: `31 * unscaledValue().hashCode() + scale`.
    pub fn java_hash_code(&self) -> i32 {
        big_integer_hash_code(&self.int_val).wrapping_mul(31).wrapping_add(self.scale)
    }

    // --------------------------------------------------------------------------------------
    // Text

    /// `toString()`: scientific notation when scale < 0 or the adjusted exponent is < -6.
    pub fn to_java_string(&self) -> String {
        self.layout_chars(true)
    }

    /// `toEngineeringString()`.
    pub fn to_engineering_string(&self) -> String {
        self.layout_chars(false)
    }

    fn layout_chars(&self, sci: bool) -> String {
        if self.scale == 0 {
            return self.int_val.to_string();
        }
        let coeff = self.int_val.magnitude().to_string();
        let coeff_len = coeff.len() as i64;
        let scale = self.scale as i64;
        let mut buf = String::with_capacity(coeff.len() + 16);
        if self.signum() < 0 {
            buf.push('-');
        }
        let mut adjusted = -scale + (coeff_len - 1);
        if scale >= 0 && adjusted >= -6 {
            let pad = scale - coeff_len;
            if pad >= 0 {
                buf.push_str("0.");
                buf.push_str(&"0".repeat(pad as usize));
                buf.push_str(&coeff);
            } else {
                let (int_part, frac) = coeff.split_at((-pad) as usize);
                buf.push_str(int_part);
                buf.push('.');
                buf.push_str(frac);
            }
            return buf;
        }
        if sci {
            buf.push_str(&coeff[..1]);
            if coeff_len > 1 {
                buf.push('.');
                buf.push_str(&coeff[1..]);
            }
        } else {
            let sig = adjusted.rem_euclid(3);
            adjusted -= sig;
            let sig = sig + 1;
            if self.is_zero() {
                match sig {
                    1 => buf.push('0'),
                    2 => {
                        buf.push_str("0.00");
                        adjusted += 3;
                    }
                    _ => {
                        buf.push_str("0.0");
                        adjusted += 3;
                    }
                }
            } else if sig >= coeff_len {
                buf.push_str(&coeff);
                buf.push_str(&"0".repeat((sig - coeff_len) as usize));
            } else {
                buf.push_str(&coeff[..sig as usize]);
                buf.push('.');
                buf.push_str(&coeff[sig as usize..]);
            }
        }
        if adjusted != 0 {
            buf.push('E');
            if adjusted > 0 {
                buf.push('+');
            }
            buf.push_str(&adjusted.to_string());
        }
        buf
    }

    /// `toPlainString()`.
    pub fn to_plain_string(&self) -> String {
        // ponytail: Java throws OutOfMemoryError / "Overflow" for |scale| near 2^31; this builds the string.
        let neg = if self.signum() < 0 { "-" } else { "" };
        let digits = self.int_val.magnitude().to_string();
        match self.scale.cmp(&0) {
            Ordering::Equal => self.int_val.to_string(),
            Ordering::Less if self.is_zero() => "0".into(),
            Ordering::Less => format!("{}{}", self.int_val, "0".repeat(self.scale.unsigned_abs() as usize)),
            Ordering::Greater => {
                let insertion = digits.len() as i64 - self.scale as i64;
                if insertion > 0 {
                    let (a, b) = digits.split_at(insertion as usize);
                    format!("{neg}{a}.{b}")
                } else {
                    format!("{neg}0.{}{digits}", "0".repeat((-insertion) as usize))
                }
            }
        }
    }

    // --------------------------------------------------------------------------------------
    // Conversions

    fn fraction_only(&self) -> bool {
        self.prec() <= self.scale as i64
    }

    /// `longValueExact()`.
    pub fn long_value_exact(&self) -> Result<i64, MathError> {
        if self.scale == 0 && is_compact(&self.int_val) {
            return Ok(self.int_val.to_i64().unwrap_or_default());
        }
        if self.is_zero() {
            return Ok(0);
        }
        if self.fraction_only() {
            return arith("Rounding necessary");
        }
        if self.prec() - 19 > self.scale as i64 {
            return arith("Overflow");
        }
        let num = self.set_scale(0, RoundingMode::Unnecessary)?;
        num.int_val.to_i64().map_or_else(|| arith("Overflow"), Ok)
    }

    /// `intValueExact()`.
    pub fn int_value_exact(&self) -> Result<i32, MathError> {
        i32::try_from(self.long_value_exact()?).or_else(|_| arith("Overflow"))
    }

    /// `longValue()`: the low 64 bits of `toBigInteger()`.
    pub fn long_value(&self) -> i64 {
        if self.is_zero() || self.fraction_only() || self.scale <= -64 {
            return 0;
        }
        let bi = self.try_to_big_integer().unwrap_or_default();
        let low = bi.magnitude().iter_u64_digits().next().unwrap_or(0);
        if bi.sign() == Sign::Minus { low.wrapping_neg() as i64 } else { low as i64 }
    }

    /// `intValue()`: the low 32 bits of `longValue()`.
    pub fn int_value(&self) -> i32 {
        self.long_value() as i32
    }

    /// `doubleValue()`: correctly rounded (JDK 21+).
    pub fn double_value(&self) -> f64 {
        const POW: [f64; 23] = [
            1e0, 1e1, 1e2, 1e3, 1e4, 1e5, 1e6, 1e7, 1e8, 1e9, 1e10, 1e11, 1e12, 1e13, 1e14, 1e15, 1e16, 1e17, 1e18,
            1e19, 1e20, 1e21, 1e22,
        ];
        if let Some(x) = self.int_val.to_i64().filter(|_| is_compact(&self.int_val)) {
            let v = x as f64;
            if self.scale == 0 {
                return v;
            }
            if v as i64 == x {
                if 0 < self.scale && self.scale < 23 {
                    return v / POW[self.scale as usize];
                }
                if -23 < self.scale && self.scale < 0 {
                    return v * POW[-self.scale as usize];
                }
            }
        }
        // ponytail: core's float parser is correctly rounded, which is what fullDoubleValue computes.
        self.exp_text().parse().unwrap_or(f64::NAN)
    }

    /// `floatValue()`: correctly rounded to float directly (JDK 21+).
    pub fn float_value(&self) -> f32 {
        const POW: [f32; 11] = [1e0, 1e1, 1e2, 1e3, 1e4, 1e5, 1e6, 1e7, 1e8, 1e9, 1e10];
        if let Some(x) = self.int_val.to_i64().filter(|_| is_compact(&self.int_val)) {
            let v = x as f32;
            if self.scale == 0 {
                return v;
            }
            if v as i64 == x {
                if 0 < self.scale && self.scale < 11 {
                    return v / POW[self.scale as usize];
                }
                if -11 < self.scale && self.scale < 0 {
                    return v * POW[-self.scale as usize];
                }
            }
        }
        self.exp_text().parse().unwrap_or(f32::NAN)
    }

    fn exp_text(&self) -> String {
        format!("{}e{}", self.int_val, -(self.scale as i64))
    }

    /// `toBigInteger()`.
    pub fn try_to_big_integer(&self) -> Result<BigInt, MathError> {
        Ok(self.set_scale(0, RoundingMode::Down)?.int_val)
    }

    /// `toBigInteger()`. Java throws "BigInteger would overflow supported range" for scales
    /// beyond about ±6.5e8; this returns 0 then (see [`Self::try_to_big_integer`]).
    pub fn to_big_integer(&self) -> BigInt {
        self.try_to_big_integer().unwrap_or_default()
    }

    /// `toBigIntegerExact()`.
    pub fn to_big_integer_exact(&self) -> Result<BigInt, MathError> {
        Ok(self.set_scale(0, RoundingMode::Unnecessary)?.int_val)
    }
}

impl fmt::Display for BigDecimal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_java_string())
    }
}

/// `divideAndRound(dividend, divisor, scale, roundingMode, preferredScale)`: an exact quotient is
/// stripped of trailing zeros down to `preferred`.
fn divide_and_round(n: &BigInt, d: &BigInt, scale: i32, rm: RoundingMode, preferred: i32) -> R {
    let (q, exact) = div_round(n, d, rm)?;
    if exact && preferred != scale {
        return strip_zeros(q, scale, preferred as i64);
    }
    BigDecimal::of(q, scale)
}

/// `createAndStripZerosToMatchScale`: removes trailing decimal zeros while scale > preferred.
fn strip_zeros(mut v: BigInt, scale: i32, preferred: i64) -> R {
    let preferred = preferred.clamp(i32::MIN as i64 - 1, i32::MAX as i64);
    let mut scale = scale as i64;
    if v.is_zero() {
        return BigDecimal::of(v, if scale > preferred { saturate(preferred) } else { scale as i32 });
    }
    let limit = (scale - preferred).min(v.magnitude().trailing_zeros().unwrap_or(0) as i64);
    let mut step = 16i64;
    let mut stripped = 0i64;
    while stripped < limit {
        let s = step.min(limit - stripped);
        let (q, r) = v.div_rem(&ten_pow(s)?);
        if r.is_zero() {
            v = q;
            stripped += s;
        } else if s == 1 {
            break;
        } else {
            step = s / 2;
        }
    }
    scale -= stripped;
    BigDecimal::of(v, check_scale(true, scale)?)
}

/// A decimal digit for BigDecimal's parser: ASCII or any Unicode Nd character (BMP).
fn decimal_digit(c: u16) -> Option<u8> {
    match char_digit(c, 10) {
        d @ 0..=9 => Some(d as u8),
        _ => None,
    }
}

/// `BigDecimal.parseExp`: `e` is the index of the 'e'/'E'; consumes the rest of the input.
fn parse_exp(u: &[u16], e: usize) -> Result<i64, MathError> {
    let null = || MathError::NumberFormat(JString::empty());
    let mut off = e + 1;
    let mut len = u.len() as i64 - off as i64;
    let mut c = *u.get(off).ok_or_else(null)?;
    let negexp = c == b'-' as u16;
    if negexp || c == b'+' as u16 {
        off += 1;
        c = *u.get(off).ok_or_else(null)?;
        len -= 1;
    }
    if len <= 0 {
        return number_format("No exponent digits.");
    }
    while len > 10 && decimal_digit(c) == Some(0) {
        off += 1;
        c = *u.get(off).ok_or_else(null)?;
        len -= 1;
    }
    if len > 10 {
        return number_format("Too many nonzero exponent digits.");
    }
    let mut exp: i64 = 0;
    loop {
        let v = decimal_digit(c).map_or_else(|| number_format("Not a digit."), Ok)?;
        exp = exp * 10 + v as i64;
        if len == 1 {
            break;
        }
        off += 1;
        c = *u.get(off).ok_or_else(null)?;
        len -= 1;
    }
    Ok(if negexp { -exp } else { exp })
}
