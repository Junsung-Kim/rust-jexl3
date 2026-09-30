// port of: org.apache.commons.jexl3.JexlArithmetic
//
// Perform arithmetic, implements JexlOperator methods. This is the class most JEXL users subclass
// to change coercions, so every Java override point is a method here.
use num_bigint::BigInt;
use num_traits::{Signed, Zero};

use crate::internal::range::{Range, Width};
use crate::java::big_decimal::{BigDecimal, MathContext, MathError};
use crate::java::number;
use crate::java::string::{JString, JStringBuilder};
use crate::value::{Value, MapKind};

/// The throwables JexlArithmetic raises, by Java class.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ArithError {
    /// JexlArithmetic.NullOperand: an ArithmeticException with a null message
    NullOperand,
    Arithmetic(JString),
    NumberFormat(JString),
    ClassCast(JString),
    NullPointer(JString),
    PatternSyntax(JString),
}

impl ArithError {
    pub fn class_name(&self) -> &'static str {
        match self {
            ArithError::NullOperand => "JexlArithmetic$NullOperand",
            ArithError::Arithmetic(_) => "java.lang.ArithmeticException",
            ArithError::NumberFormat(_) => "java.lang.NumberFormatException",
            ArithError::ClassCast(_) => "java.lang.ClassCastException",
            ArithError::NullPointer(_) => "java.lang.NullPointerException",
            ArithError::PatternSyntax(_) => "java.util.regex.PatternSyntaxException",
        }
    }

    pub fn message(&self) -> Option<JString> {
        match self {
            ArithError::NullOperand => None,
            ArithError::Arithmetic(m)
            | ArithError::NumberFormat(m)
            | ArithError::ClassCast(m)
            | ArithError::NullPointer(m)
            | ArithError::PatternSyntax(m) => Some(m.clone()),
        }
    }
}

impl From<MathError> for ArithError {
    fn from(e: MathError) -> Self {
        match e {
            MathError::Arithmetic(m) => ArithError::Arithmetic(m),
            MathError::NumberFormat(m) => ArithError::NumberFormat(m),
        }
    }
}

impl From<number::NumberFormatException> for ArithError {
    fn from(e: number::NumberFormatException) -> Self {
        ArithError::NumberFormat(JString::from(e.0))
    }
}

type R<T> = Result<T, ArithError>;

/// `NumberFormatException` whose message quotes the *original* Java String, not a lossy copy.
fn nfe_units(e: number::NumberFormatException, s: &JString) -> ArithError {
    const PREFIX: &str = "For input string: \"";
    if let Some(rest) = e.0.strip_prefix(PREFIX) {
        if let Some(close) = rest.rfind('"') {
            // only substitute when the message quotes the whole input (BigInteger quotes a slice)
            if rest[..close] == s.to_rust() {
                return ArithError::NumberFormat(
                    JStringBuilder::new().str(PREFIX).jstr(s).str("\"").str(&rest[close + 1..]).build(),
                );
            }
        }
    }
    ArithError::NumberFormat(JString::from(e.0))
}

/// The JDK's helpful NullPointerException for `object.getClass()` inside isEmpty/size.
fn npe_get_class() -> ArithError {
    ArithError::NullPointer(JString::from("Cannot invoke \"Object.getClass()\" because \"object\" is null"))
}

/// The message of the implicit checkcast in `Comparable<Object>.compareTo`.
// ponytail: both classes are assumed to be in java.base; a host object would read differently.
fn cce(from: &str, to: &str) -> ArithError {
    ArithError::ClassCast(JString::from(format!(
        "class {} cannot be cast to class {} ({} and {} are in module java.base of loader 'bootstrap')",
        from, to, from, to
    )))
}

const BIGD_SCALE: i32 = -1;

#[derive(Clone, Debug)]
pub struct JexlArithmetic {
    strict: bool,
    math_context: MathContext,
    math_scale: i32,
}

impl Default for JexlArithmetic {
    fn default() -> Self {
        JexlArithmetic::new(true, None, i32::MIN)
    }
}

impl JexlArithmetic {
    // port of: JexlArithmetic(boolean, MathContext, int)
    pub fn new(astrict: bool, bigd_context: Option<MathContext>, bigd_scale: i32) -> JexlArithmetic {
        JexlArithmetic {
            strict: astrict,
            math_context: bigd_context.unwrap_or(MathContext::DECIMAL128),
            math_scale: if bigd_scale == i32::MIN { BIGD_SCALE } else { bigd_scale },
        }
    }

    // port of: JexlArithmetic.isStrict
    pub fn is_strict(&self) -> bool {
        self.strict
    }

    // port of: JexlArithmetic.getMathContext
    pub fn get_math_context(&self) -> &MathContext {
        &self.math_context
    }

    // port of: JexlArithmetic.getMathScale
    pub fn get_math_scale(&self) -> i32 {
        self.math_scale
    }

    /// port of: JexlArithmetic.createWithOptions (options(JexlOptions) folded in)
    pub fn with_options(&self, strict: bool, ctx: Option<MathContext>, scale: i32) -> JexlArithmetic {
        let ctx = ctx.unwrap_or(self.math_context);
        let scale = if scale == i32::MIN { self.math_scale } else { scale };
        if strict != self.strict || scale != self.math_scale || ctx != self.math_context {
            return JexlArithmetic { strict, math_context: ctx, math_scale: scale };
        }
        self.clone()
    }

    // port of: JexlArithmetic.roundBigDecimal
    pub fn round_big_decimal(&self, number: &BigDecimal) -> R<BigDecimal> {
        let mscale = self.get_math_scale();
        if mscale >= 0 {
            return Ok(number.set_scale(mscale, self.math_context.rounding_mode)?);
        }
        Ok(number.clone())
    }

    // port of: JexlArithmetic.controlNullNullOperands
    fn control_null_null_operands(&self) -> R<Value> {
        if self.is_strict() {
            return Err(ArithError::NullOperand);
        }
        Ok(Value::Integer(0))
    }

    // port of: JexlArithmetic.controlNullOperand
    fn control_null_operand(&self) -> R<()> {
        if self.is_strict() {
            return Err(ArithError::NullOperand);
        }
        Ok(())
    }

    /// port of: JexlArithmetic.FLOAT_PATTERN `^[+-]?\d*(\.\d*)?([eE][+-]?\d+)?$` with the
    /// `m.start(1) >= 0 || m.start(2) >= 0` test: the text needs a fraction or an exponent.
    fn float_pattern_matches(s: &[u16]) -> bool {
        let digit = |c: u16| (b'0' as u16..=b'9' as u16).contains(&c);
        let mut i = 0usize;
        let n = s.len();
        if i < n && (s[i] == b'+' as u16 || s[i] == b'-' as u16) {
            i += 1;
        }
        while i < n && digit(s[i]) {
            i += 1;
        }
        let mut group = false;
        if i < n && s[i] == b'.' as u16 {
            i += 1;
            while i < n && digit(s[i]) {
                i += 1;
            }
            group = true;
        }
        if i < n && (s[i] == b'e' as u16 || s[i] == b'E' as u16) {
            let save = i;
            i += 1;
            if i < n && (s[i] == b'+' as u16 || s[i] == b'-' as u16) {
                i += 1;
            }
            let ds = i;
            while i < n && digit(s[i]) {
                i += 1;
            }
            if i == ds {
                // no exponent digits: the optional group does not participate
                i = save;
            } else {
                group = true;
            }
        }
        i == n && group
    }

    // port of: JexlArithmetic.isFloatingPointNumber
    pub fn is_floating_point_number(&self, val: &Value) -> bool {
        match val {
            Value::Float(_) | Value::Double(_) => true,
            Value::String(s) => Self::float_pattern_matches(s.units()),
            _ => false,
        }
    }

    // port of: JexlArithmetic.isFloatingPoint
    pub fn is_floating_point(o: &Value) -> bool {
        matches!(o, Value::Float(_) | Value::Double(_))
    }

    // port of: JexlArithmetic.isNumberable
    pub fn is_numberable(o: &Value) -> bool {
        matches!(o, Value::Integer(_) | Value::Long(_) | Value::Byte(_) | Value::Short(_) | Value::Character(_))
    }

    // port of: JexlArithmetic.narrow
    pub fn narrow(&self, original: &Value) -> Value {
        self.narrow_number(original, None)
    }

    // port of: JexlArithmetic.narrowAccept
    fn narrow_accept(narrow: Option<NumClass>, source: NumClass) -> bool {
        narrow.is_none() || narrow == Some(source)
    }

    // port of: JexlArithmetic.narrowNumber
    pub fn narrow_number(&self, original: &Value, narrow: Option<NumClass>) -> Value {
        let mut result = original.clone();
        if let Value::BigDecimal(bigd) = original {
            // if it is bigger than a double, it can not be narrowed
            if bigd.compare_to(&BigDecimal::value_of_double(f64::MAX).expect("max")) == std::cmp::Ordering::Greater
                || bigd.compare_to(&BigDecimal::value_of_double(f64::MIN_POSITIVE).expect("min")) == std::cmp::Ordering::Less
            {
                return original.clone();
            }
            if let Ok(l) = bigd.long_value_exact() {
                if Self::narrow_accept(narrow, NumClass::Integer) && l <= i32::MAX as i64 && l >= i32::MIN as i64 {
                    return Value::Integer(l as i32);
                }
                if Self::narrow_accept(narrow, NumClass::Long) {
                    return Value::Long(l);
                }
            }
        }
        if matches!(original, Value::Double(_) | Value::Float(_)) {
            let value = self.as_double(original);
            if Self::narrow_accept(narrow, NumClass::Float) && value <= f32::MAX as f64 && value >= f32::MIN_POSITIVE as f64 {
                result = Value::Float(value as f32);
            }
        } else {
            if let Value::BigInteger(bigi) = original {
                if bigi.as_ref() > &BigInt::from(i64::MAX) || bigi.as_ref() < &BigInt::from(i64::MIN) {
                    return original.clone();
                }
            }
            let value = self.as_long(original);
            if Self::narrow_accept(narrow, NumClass::Byte) && value <= i8::MAX as i64 && value >= i8::MIN as i64 {
                result = Value::Byte(value as i8);
            } else if Self::narrow_accept(narrow, NumClass::Short) && value <= i16::MAX as i64 && value >= i16::MIN as i64 {
                result = Value::Short(value as i16);
            } else if Self::narrow_accept(narrow, NumClass::Integer) && value <= i32::MAX as i64 && value >= i32::MIN as i64 {
                result = Value::Integer(value as i32);
            }
        }
        result
    }

    /// Number.doubleValue() (no coercion of non-numbers)
    fn as_double(&self, v: &Value) -> f64 {
        match v {
            Value::Byte(b) => *b as f64,
            Value::Short(s) => *s as f64,
            Value::Integer(i) => *i as f64,
            Value::Long(l) => *l as f64,
            Value::Float(f) => *f as f64,
            Value::Double(d) => *d,
            Value::BigInteger(b) => number::big_integer_double_value(b),
            Value::BigDecimal(b) => b.double_value(),
            _ => 0.0,
        }
    }

    /// Number.longValue() (no coercion of non-numbers)
    fn as_long(&self, v: &Value) -> i64 {
        match v {
            Value::Byte(b) => *b as i64,
            Value::Short(s) => *s as i64,
            Value::Integer(i) => *i as i64,
            Value::Long(l) => *l,
            Value::Float(f) => *f as i64,
            Value::Double(d) => *d as i64,
            Value::BigInteger(b) => number::big_integer_long_value(b),
            Value::BigDecimal(b) => b.long_value(),
            _ => 0,
        }
    }

    // port of: JexlArithmetic.narrowBigInteger
    fn narrow_big_integer(&self, lhs: &Value, rhs: &Value, bigi: BigInt) -> Value {
        let is_big = |v: &Value| matches!(v, Value::BigInteger(_));
        let is_long = |v: &Value| matches!(v, Value::Long(_));
        if !(is_big(lhs) || is_big(rhs)) && bigi <= BigInt::from(i64::MAX) && bigi >= BigInt::from(i64::MIN) {
            let l = number::big_integer_long_value(&bigi);
            if !(is_long(lhs) || is_long(rhs)) && l <= i32::MAX as i64 && l >= i32::MIN as i64 {
                return Value::Integer(l as i32);
            }
            return Value::Long(l);
        }
        Value::big_integer(bigi)
    }

    // port of: JexlArithmetic.narrowBigDecimal
    fn narrow_big_decimal(&self, lhs: &Value, rhs: &Value, bigd: BigDecimal) -> Value {
        if Self::is_numberable(lhs) || Self::is_numberable(rhs) {
            if let Ok(l) = bigd.long_value_exact() {
                if l <= i32::MAX as i64 && l >= i32::MIN as i64 {
                    return Value::Integer(l as i32);
                }
                return Value::Long(l);
            }
        }
        Value::big_decimal(bigd)
    }

    // port of: JexlArithmetic.narrowArguments
    pub fn narrow_arguments(&self, args: &mut [Value]) -> bool {
        let mut narrowed = false;
        for a in args.iter_mut() {
            if a.is_number() {
                let narrow = self.narrow(a);
                if !a.java_equals(&narrow) {
                    *a = narrow;
                    narrowed = true;
                }
            }
        }
        narrowed
    }

    // port of: JexlArithmetic.narrowLong
    fn narrow_long(&self, lhs: &Value, rhs: &Value, r: i64) -> Value {
        let is_long = |v: &Value| matches!(v, Value::Long(_));
        if !(is_long(lhs) || is_long(rhs)) && (r as i32) as i64 == r {
            return Value::Integer(r as i32);
        }
        Value::Long(r)
    }

    // port of: JexlArithmetic.asLongNumber
    fn as_long_number(value: &Value) -> Option<i64> {
        match value {
            Value::Long(l) => Some(*l),
            Value::Integer(i) => Some(*i as i64),
            Value::Short(s) => Some(*s as i64),
            Value::Byte(b) => Some(*b as i64),
            _ => None,
        }
    }

    fn is_big_decimal(v: &Value) -> bool {
        matches!(v, Value::BigDecimal(_))
    }

    // port of: JexlArithmetic.add
    pub fn add(&self, left: &Value, right: &Value) -> R<Value> {
        if left.is_null() && right.is_null() {
            return self.control_null_null_operands();
        }
        let strconcat = if self.strict {
            matches!(left, Value::String(_)) || matches!(right, Value::String(_))
        } else {
            matches!(left, Value::String(_)) && matches!(right, Value::String(_))
        };
        if !strconcat {
            match self.add_numeric(left, right) {
                Ok(v) => return Ok(v),
                Err(ArithError::NumberFormat(_)) => {
                    // in Java the NumberFormatException falls through to string concatenation
                    if left.is_null() || right.is_null() {
                        self.control_null_operand()?;
                    }
                }
                Err(e) => return Err(e),
            }
        }
        Ok(Value::String(self.to_jstring(left)?.concat(&self.to_jstring(right)?)))
    }

    fn add_numeric(&self, left: &Value, right: &Value) -> R<Value> {
        // if both (non-null) operands are integer-like, keep integer arithmetic
        if let (Some(x), Some(y)) = (Self::as_long_number(left), Self::as_long_number(right)) {
            let result = x.wrapping_add(y);
            // detect overflow and promote to BigInteger
            if ((x ^ result) & (y ^ result)) < 0 {
                return Ok(Value::big_integer(BigInt::from(x) + BigInt::from(y)));
            }
            return Ok(self.narrow_long(left, right, result));
        }
        if Self::is_big_decimal(left) || Self::is_big_decimal(right) {
            let l = self.to_big_decimal(left)?;
            let r = self.to_big_decimal(right)?;
            let result = l.add_mc(&r, self.get_math_context())?;
            return Ok(self.narrow_big_decimal(left, right, result));
        }
        if self.is_floating_point_number(left) || self.is_floating_point_number(right) {
            let l = self.to_double(left)?;
            let r = self.to_double(right)?;
            return Ok(Value::Double(l + r));
        }
        let l = self.to_big_integer(left)?;
        let r = self.to_big_integer(right)?;
        Ok(self.narrow_big_integer(left, right, l + r))
    }

    // port of: JexlArithmetic.divide
    pub fn divide(&self, left: &Value, right: &Value) -> R<Value> {
        if left.is_null() && right.is_null() {
            return self.control_null_null_operands();
        }
        if let (Some(x), Some(y)) = (Self::as_long_number(left), Self::as_long_number(right)) {
            if y == 0 {
                return Err(ArithError::Arithmetic(JString::from("/")));
            }
            let result = x.wrapping_div(y);
            return Ok(self.narrow_long(left, right, result));
        }
        if Self::is_big_decimal(left) || Self::is_big_decimal(right) {
            let l = self.to_big_decimal(left)?;
            let r = self.to_big_decimal(right)?;
            if BigDecimal::zero().java_equals(&r) {
                return Err(ArithError::Arithmetic(JString::from("/")));
            }
            let result = l.divide_mc(&r, self.get_math_context())?;
            return Ok(self.narrow_big_decimal(left, right, result));
        }
        if self.is_floating_point_number(left) || self.is_floating_point_number(right) {
            let l = self.to_double(left)?;
            let r = self.to_double(right)?;
            if r == 0.0 {
                return Err(ArithError::Arithmetic(JString::from("/")));
            }
            return Ok(Value::Double(l / r));
        }
        let l = self.to_big_integer(left)?;
        let r = self.to_big_integer(right)?;
        if r.is_zero() {
            return Err(ArithError::Arithmetic(JString::from("/")));
        }
        Ok(self.narrow_big_integer(left, right, l / r))
    }

    // port of: JexlArithmetic.mod
    pub fn modulo(&self, left: &Value, right: &Value) -> R<Value> {
        if left.is_null() && right.is_null() {
            return self.control_null_null_operands();
        }
        if let (Some(x), Some(y)) = (Self::as_long_number(left), Self::as_long_number(right)) {
            if y == 0 {
                return Err(ArithError::Arithmetic(JString::from("%")));
            }
            let result = x.wrapping_rem(y);
            return Ok(self.narrow_long(left, right, result));
        }
        if Self::is_big_decimal(left) || Self::is_big_decimal(right) {
            let l = self.to_big_decimal(left)?;
            let r = self.to_big_decimal(right)?;
            if BigDecimal::zero().java_equals(&r) {
                return Err(ArithError::Arithmetic(JString::from("%")));
            }
            let remainder = l.remainder_mc(&r, self.get_math_context())?;
            return Ok(self.narrow_big_decimal(left, right, remainder));
        }
        if self.is_floating_point_number(left) || self.is_floating_point_number(right) {
            let l = self.to_double(left)?;
            let r = self.to_double(right)?;
            if r == 0.0 {
                return Err(ArithError::Arithmetic(JString::from("%")));
            }
            return Ok(Value::Double(l % r));
        }
        let l = self.to_big_integer(left)?;
        let r = self.to_big_integer(right)?;
        if r.is_zero() {
            return Err(ArithError::Arithmetic(JString::from("%")));
        }
        // BigInteger.mod: always non-negative, and rejects a non-positive modulus
        if !r.is_positive() {
            return Err(ArithError::Arithmetic(JString::from("BigInteger: modulus not positive")));
        }
        Ok(self.narrow_big_integer(left, right, l.modpow(&BigInt::from(1), &r)))
    }

    // port of: JexlArithmetic.isMultiplyExact
    fn is_multiply_exact(x: i64, y: i64, r: i64) -> bool {
        let ax = x.wrapping_abs();
        let ay = y.wrapping_abs();
        !((((ax | ay) as u64) >> (i32::BITS - 1) != 0) && ((y != 0 && r.wrapping_div(y) != x) || (x == i64::MIN && y == -1)))
    }

    // port of: JexlArithmetic.multiply
    pub fn multiply(&self, left: &Value, right: &Value) -> R<Value> {
        if left.is_null() && right.is_null() {
            return self.control_null_null_operands();
        }
        if let (Some(x), Some(y)) = (Self::as_long_number(left), Self::as_long_number(right)) {
            let result = x.wrapping_mul(y);
            if !Self::is_multiply_exact(x, y, result) {
                return Ok(Value::big_integer(BigInt::from(x) * BigInt::from(y)));
            }
            return Ok(self.narrow_long(left, right, result));
        }
        if Self::is_big_decimal(left) || Self::is_big_decimal(right) {
            let l = self.to_big_decimal(left)?;
            let r = self.to_big_decimal(right)?;
            let result = l.multiply_mc(&r, self.get_math_context())?;
            return Ok(self.narrow_big_decimal(left, right, result));
        }
        if self.is_floating_point_number(left) || self.is_floating_point_number(right) {
            let l = self.to_double(left)?;
            let r = self.to_double(right)?;
            return Ok(Value::Double(l * r));
        }
        let l = self.to_big_integer(left)?;
        let r = self.to_big_integer(right)?;
        Ok(self.narrow_big_integer(left, right, l * r))
    }

    // port of: JexlArithmetic.subtract
    pub fn subtract(&self, left: &Value, right: &Value) -> R<Value> {
        if left.is_null() && right.is_null() {
            return self.control_null_null_operands();
        }
        if let (Some(x), Some(y)) = (Self::as_long_number(left), Self::as_long_number(right)) {
            let result = x.wrapping_sub(y);
            if ((x ^ y) & (x ^ result)) < 0 {
                return Ok(Value::big_integer(BigInt::from(x) - BigInt::from(y)));
            }
            return Ok(self.narrow_long(left, right, result));
        }
        if Self::is_big_decimal(left) || Self::is_big_decimal(right) {
            let l = self.to_big_decimal(left)?;
            let r = self.to_big_decimal(right)?;
            let result = l.subtract_mc(&r, self.get_math_context())?;
            return Ok(self.narrow_big_decimal(left, right, result));
        }
        if self.is_floating_point_number(left) || self.is_floating_point_number(right) {
            let l = self.to_double(left)?;
            let r = self.to_double(right)?;
            return Ok(Value::Double(l - r));
        }
        let l = self.to_big_integer(left)?;
        let r = self.to_big_integer(right)?;
        Ok(self.narrow_big_integer(left, right, l - r))
    }

    // port of: JexlArithmetic.negate
    pub fn negate(&self, val: &Value) -> R<Value> {
        match val {
            Value::Null => {
                self.control_null_operand()?;
                Ok(Value::Null)
            }
            Value::Integer(i) => Ok(Value::Integer(i.wrapping_neg())),
            Value::Double(d) => Ok(Value::Double(-d)),
            Value::Long(l) => Ok(Value::Long(l.wrapping_neg())),
            Value::BigDecimal(b) => Ok(Value::big_decimal(b.negate())),
            Value::BigInteger(b) => Ok(Value::big_integer(-b.as_ref().clone())),
            Value::Float(f) => Ok(Value::Float(-f)),
            Value::Short(s) => Ok(Value::Short(s.wrapping_neg())),
            Value::Byte(b) => Ok(Value::Byte(b.wrapping_neg())),
            Value::Boolean(b) => Ok(Value::Boolean(!b)),
            Value::AtomicBoolean(b) => Ok(Value::Boolean(!b.load(std::sync::atomic::Ordering::SeqCst))),
            other => Err(ArithError::Arithmetic(
                JStringBuilder::new().str("Object negate:(").jstr(&other.java_to_jstring()).str(")").build(),
            )),
        }
    }

    // port of: JexlArithmetic.isNegateStable
    pub fn is_negate_stable(&self) -> bool {
        true
    }

    // port of: JexlArithmetic.positivize
    pub fn positivize(&self, val: &Value) -> R<Value> {
        match val {
            Value::Null => {
                self.control_null_operand()?;
                Ok(Value::Null)
            }
            Value::Short(s) => Ok(Value::Integer(*s as i32)),
            Value::Byte(b) => Ok(Value::Integer(*b as i32)),
            v if v.is_number() => Ok(v.clone()),
            Value::Character(c) => Ok(Value::Integer(*c as i32)),
            Value::Boolean(_) => Ok(val.clone()),
            Value::AtomicBoolean(b) => Ok(Value::Boolean(b.load(std::sync::atomic::Ordering::SeqCst))),
            other => Err(ArithError::Arithmetic(
                JStringBuilder::new().str("Object positivize:(").jstr(&other.java_to_jstring()).str(")").build(),
            )),
        }
    }

    // port of: JexlArithmetic.isPositivizeStable
    pub fn is_positivize_stable(&self) -> bool {
        true
    }

    // port of: JexlArithmetic.contains
    pub fn contains(&self, container: &Value, value: &Value) -> R<Option<bool>> {
        if value.is_null() && container.is_null() {
            // if both are null L == R
            return Ok(Some(true));
        }
        if value.is_null() || container.is_null() {
            // we know both aren't null, therefore L != R
            return Ok(Some(false));
        }
        // use regexp exclusively
        if let Some(p) = container.as_host::<crate::java::regex::Pattern>() {
            return Ok(Some(p.matches(&value.java_to_jstring().to_rust())));
        }
        if let Value::String(c) = container {
            let v = value.java_to_jstring();
            return match crate::java::regex::string_matches(&v.to_rust(), &c.to_rust()) {
                Ok(b) => Ok(Some(b)),
                Err(e) => Err(ArithError::PatternSyntax(JString::from(e.get_message()))),
            };
        }
        if let Value::Map(m) = container {
            if let Value::Map(v) = value {
                // the keys of the value map must all be keys of the container
                let keys = v.snapshot();
                return Ok(Some(keys.iter().all(|(k, _)| m.contains_key(k))));
            }
            return Ok(Some(m.contains_key(value)));
        }
        if is_collection(container) {
            if is_collection(value) {
                // Collection.containsAll: every element of the value is in the container
                let mut all = true;
                for_each_element(value, |e| {
                    if all && !collection_contains(container, &e) {
                        all = false;
                    }
                    all
                });
                return Ok(Some(all));
            }
            return Ok(Some(collection_contains(container, value)));
        }
        Ok(None)
    }

    // port of: JexlArithmetic.endsWith
    pub fn ends_with(&self, left: &Value, right: &Value) -> R<Option<bool>> {
        if left.is_null() && right.is_null() {
            return Ok(Some(true));
        }
        if left.is_null() || right.is_null() {
            return Ok(Some(false));
        }
        if let Value::String(_) = left {
            return Ok(Some(self.to_jstring(left)?.ends_with(&self.to_jstring(right)?)));
        }
        Ok(None)
    }

    // port of: JexlArithmetic.startsWith
    pub fn starts_with(&self, left: &Value, right: &Value) -> R<Option<bool>> {
        if left.is_null() && right.is_null() {
            return Ok(Some(true));
        }
        if left.is_null() || right.is_null() {
            return Ok(Some(false));
        }
        if let Value::String(_) = left {
            return Ok(Some(self.to_jstring(left)?.starts_with(&self.to_jstring(right)?)));
        }
        Ok(None)
    }

    // port of: JexlArithmetic.empty
    pub fn empty(&self, object: &Value) -> R<bool> {
        if object.is_null() {
            return Ok(true);
        }
        Ok(self.is_empty(object, Some(false))?.unwrap_or(false))
    }

    // port of: JexlArithmetic.isEmpty(Object, Boolean)
    pub fn is_empty(&self, object: &Value, def: Option<bool>) -> R<Option<bool>> {
        match object {
            v if v.is_number() => {
                let d = self.as_double(v);
                Ok(Some(d.is_nan() || d == 0.0))
            }
            Value::String(s) => Ok(Some(s.is_empty())),
            Value::Null => Err(npe_get_class()),
            Value::Array(a) => Ok(Some(a.is_empty())),
            Value::List(l) => Ok(Some(l.is_empty())),
            Value::Set(s) => Ok(Some(s.is_empty())),
            Value::Object(o) => match o.as_any().downcast_ref::<Range>() {
                // IntegerRange/LongRange are Collections whose isEmpty() is always false
                Some(_) => Ok(Some(false)),
                None => Ok(def),
            },
            Value::Map(m) => Ok(Some(m.is_empty())),
            _ => Ok(def),
        }
    }

    // port of: JexlArithmetic.size(Object, Integer)
    pub fn size(&self, object: &Value, def: Option<i32>) -> R<Option<i32>> {
        match object {
            Value::String(s) => Ok(Some(s.len() as i32)),
            Value::Null => Err(npe_get_class()),
            Value::Array(a) => Ok(Some(a.len() as i32)),
            Value::List(l) => Ok(Some(l.len() as i32)),
            Value::Set(s) => Ok(Some(s.len() as i32)),
            Value::Object(o) => match o.as_any().downcast_ref::<Range>() {
                Some(r) => Ok(Some(r.size())),
                None => Ok(def),
            },
            Value::Map(m) => Ok(Some(m.len() as i32)),
            _ => Ok(def),
        }
    }

    // port of: JexlArithmetic.and
    pub fn and(&self, left: &Value, right: &Value) -> R<Value> {
        Ok(Value::Long(self.to_long(left)? & self.to_long(right)?))
    }

    // port of: JexlArithmetic.or
    pub fn or(&self, left: &Value, right: &Value) -> R<Value> {
        Ok(Value::Long(self.to_long(left)? | self.to_long(right)?))
    }

    // port of: JexlArithmetic.xor
    pub fn xor(&self, left: &Value, right: &Value) -> R<Value> {
        Ok(Value::Long(self.to_long(left)? ^ self.to_long(right)?))
    }

    // port of: JexlArithmetic.complement
    pub fn complement(&self, val: &Value) -> R<Value> {
        Ok(Value::Long(!self.to_long(val)?))
    }

    // port of: JexlArithmetic.not
    pub fn not(&self, val: &Value) -> R<Value> {
        Ok(Value::Boolean(!self.to_boolean(val)?))
    }

    /// The Java class of a Comparable value, for the checkcast message.
    fn comparable_class(v: &Value) -> Option<&'static str> {
        match v {
            Value::Boolean(_) => Some("java.lang.Boolean"),
            Value::Byte(_) => Some("java.lang.Byte"),
            Value::Short(_) => Some("java.lang.Short"),
            Value::Integer(_) => Some("java.lang.Integer"),
            Value::Long(_) => Some("java.lang.Long"),
            Value::Float(_) => Some("java.lang.Float"),
            Value::Double(_) => Some("java.lang.Double"),
            Value::Character(_) => Some("java.lang.Character"),
            Value::String(_) => Some("java.lang.String"),
            Value::BigInteger(_) => Some("java.math.BigInteger"),
            Value::BigDecimal(_) => Some("java.math.BigDecimal"),
            _ => None,
        }
    }

    /// Comparable.compareTo between two values of the same Java class.
    fn compare_same(left: &Value, right: &Value) -> Option<i32> {
        match (left, right) {
            (Value::Boolean(a), Value::Boolean(b)) => Some(if a == b {
                0
            } else if *a {
                1
            } else {
                -1
            }),
            (Value::Character(a), Value::Character(b)) => Some(*a as i32 - *b as i32),
            _ => None,
        }
    }

    // port of: JexlArithmetic.compare
    pub fn compare(&self, left: &Value, right: &Value, operator: &str) -> R<i32> {
        if !left.is_null() && !right.is_null() {
            if Self::is_big_decimal(left) || Self::is_big_decimal(right) {
                let l = self.to_big_decimal(left)?;
                let r = self.to_big_decimal(right)?;
                return Ok(ord(l.compare_to(&r)));
            }
            if matches!(left, Value::BigInteger(_)) || matches!(right, Value::BigInteger(_)) {
                let l = self.to_big_integer(left)?;
                let r = self.to_big_integer(right)?;
                return Ok(ord(l.cmp(&r)));
            }
            if Self::is_floating_point(left) || Self::is_floating_point(right) {
                let lhs = self.to_double(left)?;
                let rhs = self.to_double(right)?;
                if lhs.is_nan() {
                    if rhs.is_nan() {
                        return Ok(0);
                    }
                    return Ok(-1);
                }
                if rhs.is_nan() {
                    return Ok(1);
                }
                if lhs < rhs {
                    return Ok(-1);
                }
                if lhs > rhs {
                    return Ok(1);
                }
                return Ok(0);
            }
            if Self::is_numberable(left) || Self::is_numberable(right) {
                let lhs = self.to_long(left)?;
                let rhs = self.to_long(right)?;
                if lhs < rhs {
                    return Ok(-1);
                }
                if lhs > rhs {
                    return Ok(1);
                }
                return Ok(0);
            }
            if matches!(left, Value::String(_)) || matches!(right, Value::String(_)) {
                return Ok(self.to_jstring(left)?.compare_to(&self.to_jstring(right)?));
            }
            if operator == "==" {
                return Ok(if left.java_equals(right) { 0 } else { -1 });
            }
            if let Some(lc) = Self::comparable_class(left) {
                return match Self::compare_same(left, right) {
                    Some(c) => Ok(c),
                    None => Err(cce(&right.class_name(), lc)),
                };
            }
            if let Some(rc) = Self::comparable_class(right) {
                return match Self::compare_same(right, left) {
                    Some(c) => Ok(c),
                    None => Err(cce(&left.class_name(), rc)),
                };
            }
        }
        Err(ArithError::Arithmetic(
            JStringBuilder::new()
                .str("Object comparison:(")
                .jstr(&left.java_to_jstring())
                .str(" ")
                .str(operator)
                .str(" ")
                .jstr(&right.java_to_jstring())
                .str(")")
                .build(),
        ))
    }

    // port of: JexlArithmetic.equals
    pub fn equals(&self, left: &Value, right: &Value) -> R<bool> {
        if left.same_instance(right) {
            return Ok(true);
        }
        if left.is_null() || right.is_null() {
            return Ok(false);
        }
        if matches!(left, Value::Boolean(_)) || matches!(right, Value::Boolean(_)) {
            return Ok(self.to_boolean(left)? == self.to_boolean(right)?);
        }
        Ok(self.compare(left, right, "==")? == 0)
    }

    // port of: JexlArithmetic.lessThan
    pub fn less_than(&self, left: &Value, right: &Value) -> R<bool> {
        if left.same_instance(right) || left.is_null() || right.is_null() {
            return Ok(false);
        }
        Ok(self.compare(left, right, "<")? < 0)
    }

    // port of: JexlArithmetic.greaterThan
    pub fn greater_than(&self, left: &Value, right: &Value) -> R<bool> {
        if left.same_instance(right) || left.is_null() || right.is_null() {
            return Ok(false);
        }
        Ok(self.compare(left, right, ">")? > 0)
    }

    // port of: JexlArithmetic.lessThanOrEqual
    pub fn less_than_or_equal(&self, left: &Value, right: &Value) -> R<bool> {
        if left.same_instance(right) {
            return Ok(true);
        }
        if left.is_null() || right.is_null() {
            return Ok(false);
        }
        Ok(self.compare(left, right, "<=")? <= 0)
    }

    // port of: JexlArithmetic.greaterThanOrEqual
    pub fn greater_than_or_equal(&self, left: &Value, right: &Value) -> R<bool> {
        if left.same_instance(right) {
            return Ok(true);
        }
        if left.is_null() || right.is_null() {
            return Ok(false);
        }
        Ok(self.compare(left, right, ">=")? >= 0)
    }

    // port of: JexlArithmetic.toBoolean
    pub fn to_boolean(&self, val: &Value) -> R<bool> {
        match val {
            Value::Null => {
                self.control_null_operand()?;
                Ok(false)
            }
            Value::Boolean(b) => Ok(*b),
            v if v.is_number() => {
                let number = self.to_double(v)?;
                Ok(!number.is_nan() && number != 0.0)
            }
            Value::AtomicBoolean(b) => Ok(b.load(std::sync::atomic::Ordering::SeqCst)),
            Value::String(s) => Ok(!s.is_empty() && !s.eq_str("false")),
            _ => Ok(true),
        }
    }

    // port of: JexlArithmetic.toInteger
    pub fn to_integer(&self, val: &Value) -> R<i32> {
        match val {
            Value::Null => {
                self.control_null_operand()?;
                Ok(0)
            }
            Value::Double(d) => Ok(if d.is_nan() { 0 } else { *d as i32 }),
            Value::Byte(b) => Ok(*b as i32),
            Value::Short(s) => Ok(*s as i32),
            Value::Integer(i) => Ok(*i),
            Value::Long(l) => Ok(*l as i32),
            Value::Float(f) => Ok(*f as i32),
            Value::BigInteger(b) => Ok(number::big_integer_int_value(b)),
            Value::BigDecimal(b) => Ok(b.int_value()),
            Value::String(s) => {
                if s.is_empty() {
                    return Ok(0);
                }
                number::parse_int(&s.to_rust(), 10).map_err(|e| nfe_units(e, s))
            }
            Value::Boolean(b) => Ok(if *b { 1 } else { 0 }),
            Value::AtomicBoolean(b) => Ok(if b.load(std::sync::atomic::Ordering::SeqCst) { 1 } else { 0 }),
            Value::Character(c) => Ok(*c as i32),
            other => Err(ArithError::Arithmetic(
                JStringBuilder::new()
                    .str("Integer coercion: ")
                    .str(&other.class_name())
                    .str(":(")
                    .jstr(&other.java_to_jstring())
                    .str(")")
                    .build(),
            )),
        }
    }

    // port of: JexlArithmetic.toLong
    pub fn to_long(&self, val: &Value) -> R<i64> {
        match val {
            Value::Null => {
                self.control_null_operand()?;
                Ok(0)
            }
            Value::Double(d) => Ok(if d.is_nan() { 0 } else { *d as i64 }),
            Value::Byte(b) => Ok(*b as i64),
            Value::Short(s) => Ok(*s as i64),
            Value::Integer(i) => Ok(*i as i64),
            Value::Long(l) => Ok(*l),
            Value::Float(f) => Ok(*f as i64),
            Value::BigInteger(b) => Ok(number::big_integer_long_value(b)),
            Value::BigDecimal(b) => Ok(b.long_value()),
            Value::String(s) => {
                if s.is_empty() {
                    return Ok(0);
                }
                number::parse_long(&s.to_rust(), 10).map_err(|e| nfe_units(e, s))
            }
            Value::Boolean(b) => Ok(if *b { 1 } else { 0 }),
            Value::AtomicBoolean(b) => Ok(if b.load(std::sync::atomic::Ordering::SeqCst) { 1 } else { 0 }),
            Value::Character(c) => Ok(*c as i64),
            other => Err(ArithError::Arithmetic(
                JStringBuilder::new()
                    .str("Long coercion: ")
                    .str(&other.class_name())
                    .str(":(")
                    .jstr(&other.java_to_jstring())
                    .str(")")
                    .build(),
            )),
        }
    }

    // port of: JexlArithmetic.toBigInteger
    pub fn to_big_integer(&self, val: &Value) -> R<BigInt> {
        match val {
            Value::Null => {
                self.control_null_operand()?;
                Ok(BigInt::from(0))
            }
            Value::BigInteger(b) => Ok(b.as_ref().clone()),
            Value::Double(d) => {
                if d.is_nan() {
                    return Ok(BigInt::from(0));
                }
                Ok(BigInt::from(*d as i64))
            }
            Value::BigDecimal(b) => Ok(b.to_big_integer()),
            v if v.is_number() => Ok(BigInt::from(self.as_long(v))),
            Value::Boolean(b) => Ok(BigInt::from(if *b { 1 } else { 0 })),
            Value::AtomicBoolean(b) => Ok(BigInt::from(if b.load(std::sync::atomic::Ordering::SeqCst) { 1 } else { 0 })),
            Value::String(s) => {
                if s.is_empty() {
                    return Ok(BigInt::from(0));
                }
                number::parse_big_integer(&s.to_rust(), 10).map_err(|e| nfe_units(e, s))
            }
            Value::Character(c) => Ok(BigInt::from(*c as i32)),
            other => Err(ArithError::Arithmetic(
                JStringBuilder::new()
                    .str("BigInteger coercion: ")
                    .str(&other.class_name())
                    .str(":(")
                    .jstr(&other.java_to_jstring())
                    .str(")")
                    .build(),
            )),
        }
    }

    // port of: JexlArithmetic.toBigDecimal
    pub fn to_big_decimal(&self, val: &Value) -> R<BigDecimal> {
        match val {
            Value::BigDecimal(b) => self.round_big_decimal(b),
            Value::Null => {
                self.control_null_operand()?;
                Ok(BigDecimal::zero())
            }
            Value::Double(d) => {
                if d.is_nan() {
                    return Ok(BigDecimal::zero());
                }
                let parsed = BigDecimal::parse_with(&number::double_to_string(*d), self.get_math_context())?;
                self.round_big_decimal(&parsed)
            }
            v if v.is_number() => {
                let parsed = BigDecimal::parse_with(&v.java_to_string(), self.get_math_context())?;
                self.round_big_decimal(&parsed)
            }
            Value::Boolean(b) => Ok(BigDecimal::value_of_double(if *b { 1.0 } else { 0.0 })?),
            Value::AtomicBoolean(b) => Ok(BigDecimal::from_i64(if b.load(std::sync::atomic::Ordering::SeqCst) { 1 } else { 0 })),
            Value::String(s) => {
                if s.is_empty() {
                    return Ok(BigDecimal::zero());
                }
                let parsed = BigDecimal::parse_units_with(s.units(), self.get_math_context())?;
                self.round_big_decimal(&parsed)
            }
            Value::Character(c) => Ok(BigDecimal::from_i64(*c as i64)),
            other => Err(ArithError::Arithmetic(
                JStringBuilder::new()
                    .str("BigDecimal coercion: ")
                    .str(&other.class_name())
                    .str(":(")
                    .jstr(&other.java_to_jstring())
                    .str(")")
                    .build(),
            )),
        }
    }

    // port of: JexlArithmetic.toDouble
    pub fn to_double(&self, val: &Value) -> R<f64> {
        match val {
            Value::Null => {
                self.control_null_operand()?;
                Ok(0.0)
            }
            Value::Double(d) => Ok(*d),
            // Java: Double.parseDouble(String.valueOf(number)) for every other Number
            v if v.is_number() => Ok(number::parse_double(&v.java_to_string())?),
            Value::Boolean(b) => Ok(if *b { 1.0 } else { 0.0 }),
            Value::AtomicBoolean(b) => Ok(if b.load(std::sync::atomic::Ordering::SeqCst) { 1.0 } else { 0.0 }),
            Value::String(s) => {
                if s.is_empty() {
                    return Ok(f64::NAN);
                }
                number::parse_double(&s.to_rust()).map_err(|e| nfe_units(e, s))
            }
            Value::Character(c) => Ok(*c as i32 as f64),
            other => Err(ArithError::Arithmetic(
                JStringBuilder::new()
                    .str("Double coercion: ")
                    .str(&other.class_name())
                    .str(":(")
                    .jstr(&other.java_to_jstring())
                    .str(")")
                    .build(),
            )),
        }
    }

    /// port of: JexlArithmetic.toString (a Java String, so UTF-16)
    pub fn to_jstring(&self, val: &Value) -> R<JString> {
        match val {
            Value::Null => {
                self.control_null_operand()?;
                Ok(JString::empty())
            }
            Value::Double(d) => {
                if d.is_nan() {
                    return Ok(JString::empty());
                }
                Ok(JString::from(number::double_to_string(*d)))
            }
            other => Ok(other.java_to_jstring()),
        }
    }

    // port of: JexlArithmetic.createRange
    pub fn create_range(&self, from: &Value, to: &Value) -> R<Range> {
        let lfrom = self.to_long(from)?;
        let lto = self.to_long(to)?;
        if (i32::MIN as i64..=i32::MAX as i64).contains(&lfrom) && (i32::MIN as i64..=i32::MAX as i64).contains(&lto) {
            return Ok(Range::create(Width::Integer, lfrom, lto));
        }
        Ok(Range::create(Width::Long, lfrom, lto))
    }

    /// port of: JexlArithmetic.arrayBuilder / setBuilder / mapBuilder result kinds
    pub fn map_kind(&self) -> MapKind {
        MapKind::HashMap
    }
}

/// The Java class the narrowing methods accept.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NumClass {
    Byte,
    Short,
    Integer,
    Long,
    Float,
    Double,
}

fn ord(o: std::cmp::Ordering) -> i32 {
    match o {
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Equal => 0,
        std::cmp::Ordering::Greater => 1,
    }
}

/// Whether the value is a java.util.Collection (arrays are NOT Collections in Java).
fn is_collection(v: &Value) -> bool {
    match v {
        Value::List(_) | Value::Set(_) => true,
        Value::Object(o) => o.as_any().downcast_ref::<Range>().is_some(),
        _ => false,
    }
}

/// Collection.contains
fn collection_contains(container: &Value, e: &Value) -> bool {
    match container {
        Value::List(l) => l.snapshot().iter().any(|c| c.java_equals(e)),
        Value::Set(s) => s.contains(e),
        Value::Object(o) => match o.as_any().downcast_ref::<Range>() {
            Some(r) => r.contains(e),
            None => false,
        },
        _ => false,
    }
}

/// Iterates a Collection value, stopping when the visitor returns false.
fn for_each_element(v: &Value, mut f: impl FnMut(Value) -> bool) {
    match v {
        Value::List(l) => {
            for e in l.snapshot() {
                if !f(e) {
                    return;
                }
            }
        }
        Value::Set(s) => {
            for e in s.snapshot() {
                if !f(e) {
                    return;
                }
            }
        }
        Value::Object(o) => {
            if let Some(r) = o.as_any().downcast_ref::<Range>() {
                for e in r.iter() {
                    if !f(e) {
                        return;
                    }
                }
            }
        }
        _ => {}
    }
}
