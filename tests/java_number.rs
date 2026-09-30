// port of: (tests) java.lang.Double/Float/Integer/Long/Short/Byte, java.math.BigInteger/BigDecimal
// Replays JVM-generated fixtures from tools/javagen/NumberGen.java (Corretto 25).
// Set JAVA_NUMBER_DIR to replay a bigger locally generated campaign instead of tests/data/java_number.
use num_bigint::BigInt;
use rust_jexl3::java::big_decimal::{BigDecimal, MathContext, MathError, RoundingMode};
use rust_jexl3::java::number::*;
use std::path::PathBuf;

fn data_dir() -> PathBuf {
    match std::env::var("JAVA_NUMBER_DIR") {
        Ok(d) => PathBuf::from(d),
        Err(_) => PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data/java_number"),
    }
}

fn lines(name: &str) -> Vec<String> {
    let p = data_dir().join(name);
    let s = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
    s.lines().map(str::to_owned).collect()
}

/// Inverse of NumberGen.esc: `\\` and `\uXXXX` (UTF-16 code units).
fn unesc(s: &str) -> String {
    let b = s.as_bytes();
    let mut units: Vec<u16> = Vec::with_capacity(s.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\\' && b.get(i + 1) == Some(&b'\\') {
            units.push(b'\\' as u16);
            i += 2;
        } else if b[i] == b'\\' && b.get(i + 1) == Some(&b'u') {
            units.push(u16::from_str_radix(&s[i + 2..i + 6], 16).unwrap());
            i += 6;
        } else {
            units.push(b[i] as u16);
            i += 1;
        }
    }
    String::from_utf16_lossy(&units)
}

/// Decodes an expected field: `{s}` stands for the (escaped) input of the line.
fn expected(field: &str, raw_input: &str) -> String {
    unesc(&field.replace("{s}", raw_input))
}

struct Check {
    name: &'static str,
    count: usize,
    failures: Vec<String>,
}

impl Check {
    fn new(name: &'static str) -> Self {
        Check { name, count: 0, failures: Vec::new() }
    }
    fn eq(&mut self, what: &str, input: &str, got: &str, want: &str) {
        self.count += 1;
        if got != want && self.failures.len() < 10_000 {
            self.failures.push(format!("{what}({input:?}): got {got:?} want {want:?}"));
        }
    }
    fn finish(self) {
        eprintln!("{}: {} checks, {} failures", self.name, self.count, self.failures.len());
        for f in self.failures.iter().take(40) {
            eprintln!("  {f}");
        }
        assert!(self.failures.is_empty(), "{}: {} failures", self.name, self.failures.len());
    }
}

fn nfe<T: ToString>(r: Result<T, NumberFormatException>) -> String {
    match r {
        Ok(v) => v.to_string(),
        Err(NumberFormatException(m)) => format!("EX:N:{m}"),
    }
}

fn me(e: &MathError) -> String {
    match e {
        MathError::Arithmetic(m) => format!("EX:A:{m}"),
        MathError::NumberFormat(m) => format!("EX:N:{m}"),
    }
}

fn bd(r: Result<BigDecimal, MathError>) -> String {
    match r {
        Ok(v) => v.to_java_string(),
        Err(e) => me(&e),
    }
}

fn res<T: ToString>(r: Result<T, MathError>) -> String {
    match r {
        Ok(v) => v.to_string(),
        Err(e) => me(&e),
    }
}

fn parse_bd(s: &str) -> BigDecimal {
    BigDecimal::parse(s).unwrap_or_else(|e| panic!("fixture operand {s:?}: {e:?}"))
}

fn rm(s: &str) -> RoundingMode {
    RoundingMode::value_of(s).unwrap_or_else(|| panic!("rounding mode {s}"))
}

fn mc(prec: &str, mode: &str) -> MathContext {
    MathContext { precision: prec.parse().unwrap(), rounding_mode: rm(mode) }
}

#[test]
fn double_to_string_hash_and_big_decimal() {
    let mut c = Check::new("double");
    for l in lines("double.txt") {
        let f: Vec<&str> = l.split('\t').collect();
        let bits = u64::from_str_radix(f[0], 16).unwrap();
        let d = f64::from_bits(bits);
        c.eq("Double.toString", f[0], &double_to_string(d), f[1]);
        c.eq("Double.hashCode", f[0], &double_hash_code(d).to_string(), f[2]);
        c.eq("BigDecimal.valueOf(double)", f[0], &bd(BigDecimal::value_of_double(d)), &unesc(f[3]));
        let exact = match BigDecimal::from_double_exact(d) {
            Ok(e) => {
                let mut s = format!(
                    "{}/{}/{}/{:x}/{:x}",
                    e.scale(),
                    e.precision(),
                    e.java_hash_code(),
                    e.float_value().to_bits(),
                    e.double_value().to_bits()
                );
                if e.precision() < 40 {
                    s = format!("{s}/{}", e.to_java_string());
                }
                s
            }
            Err(e) => me(&e),
        };
        c.eq("new BigDecimal(double)", f[0], &exact, &unesc(f[4]));
        c.eq("Long.hashCode", f[0], &long_hash_code(bits as i64).to_string(), f[5]);
        // Round trip: Double.parseDouble(Double.toString(d)) == d (canonical NaN).
        let back = parse_double(f[1]).map(|v| v.to_bits()).unwrap_or(0xdead);
        let want = if d.is_nan() { 0x7ff8_0000_0000_0000 } else { bits };
        c.eq("parseDouble(toString)", f[1], &format!("{back:x}"), &format!("{want:x}"));
    }
    c.finish();
}

#[test]
fn float_to_string_and_hash() {
    let mut c = Check::new("float");
    for l in lines("float.txt") {
        let f: Vec<&str> = l.split('\t').collect();
        let bits = u32::from_str_radix(f[0], 16).unwrap();
        let v = f32::from_bits(bits);
        c.eq("Float.toString", f[0], &float_to_string(v), f[1]);
        c.eq("Float.hashCode", f[0], &float_hash_code(v).to_string(), f[2]);
        c.eq("Double.toString((double) f)", f[0], &double_to_string(v as f64), f[3]);
        let back = parse_float(f[1]).map(|v| v.to_bits()).unwrap_or(0xdead);
        let want = if v.is_nan() { 0x7fc0_0000 } else { bits };
        c.eq("parseFloat(toString)", f[1], &format!("{back:x}"), &format!("{want:x}"));
    }
    c.finish();
}

#[test]
fn parse_double_and_float() {
    let mut c = Check::new("parse_fp");
    for l in lines("parse_fp.txt") {
        let f: Vec<&str> = l.split('\t').collect();
        let s = unesc(f[0]);
        let d = match parse_double(&s) {
            Ok(v) => format!("{:x}", v.to_bits()),
            Err(NumberFormatException(m)) => format!("EX:N:{m}"),
        };
        c.eq("Double.parseDouble", &s, &d, &expected(f[1], f[0]));
        let fl = match parse_float(&s) {
            Ok(v) => format!("{:x}", v.to_bits()),
            Err(NumberFormatException(m)) => format!("EX:N:{m}"),
        };
        c.eq("Float.parseFloat", &s, &fl, &expected(f[2], f[0]));
    }
    c.finish();
}

#[test]
fn parse_integers() {
    let mut c = Check::new("parse_int");
    for l in lines("parse_int.txt") {
        let f: Vec<&str> = l.split('\t').collect();
        let s = unesc(f[0]);
        let radix: u32 = f[1].parse().unwrap();
        c.eq("Integer.parseInt", &s, &nfe(parse_int(&s, radix)), &expected(f[2], f[0]));
        c.eq("Long.parseLong", &s, &nfe(parse_long(&s, radix)), &expected(f[3], f[0]));
        c.eq("Short.parseShort", &s, &nfe(parse_short(&s, radix)), &expected(f[4], f[0]));
        c.eq("Byte.parseByte", &s, &nfe(parse_byte(&s, radix)), &expected(f[5], f[0]));
        let big = nfe(parse_big_integer(&s, radix).map(|b| big_integer_hash_code(&b)));
        c.eq("new BigInteger(s, radix)", &s, &big, &expected(f[6], f[0]));
        if let Ok(b) = parse_big_integer(&s, radix) {
            let r = if (2..=36).contains(&radix) { radix } else { 10 };
            let back = parse_big_integer(&big_integer_to_string(&b, r), r).ok();
            c.eq("BigInteger toString round trip", &s, &format!("{back:?}"), &format!("{:?}", Some(b)));
        }
    }
    c.finish();
}

#[test]
fn character_digit_table() {
    let mut c = Check::new("char_digit");
    let mut want = vec![-1i32; 0x10000];
    for l in lines("char_digit.txt") {
        let f: Vec<&str> = l.split(' ').collect();
        let (a, b) = (usize::from_str_radix(f[0], 16).unwrap(), usize::from_str_radix(f[1], 16).unwrap());
        let v0: i32 = f[2].parse().unwrap();
        for (i, w) in want.iter_mut().enumerate().take(b + 1).skip(a) {
            *w = v0 + (i - a) as i32;
        }
    }
    for (u, &w) in want.iter().enumerate() {
        for radix in [2u32, 10, 16, 36] {
            let w = if w < radix as i32 { w } else { -1 };
            c.eq("Character.digit", &format!("{u:04x}/{radix}"), &char_digit(u as u16, radix).to_string(), &w.to_string());
        }
    }
    c.finish();
}

#[test]
fn big_decimal_parse() {
    let mut c = Check::new("bd_parse");
    for l in lines("bd_parse.txt") {
        let f: Vec<&str> = l.split('\t').collect();
        let s = unesc(f[0]);
        c.eq("new BigDecimal(String)", &s, &bd(BigDecimal::parse(&s)), &expected(f[1], f[0]));
        let m = mc(f[2], f[3]);
        c.eq("new BigDecimal(String, mc)", &s, &bd(BigDecimal::parse_with(&s, &m)), &expected(f[4], f[0]));
        c.eq("hashCode", &s, &res(BigDecimal::parse(&s).map(|v| v.java_hash_code())), &expected(f[5], f[0]));
    }
    c.finish();
}

#[test]
fn big_decimal_unary() {
    let mut c = Check::new("bd_unary");
    for l in lines("bd_unary.txt") {
        let f: Vec<&str> = l.split('\t').collect();
        let a = parse_bd(f[0]);
        let m = mc(f[1], f[2]);
        let k: i32 = f[3].parse().unwrap();
        let r = rm(f[4]);
        let pw: i32 = f[5].parse().unwrap();
        let i = f[0];
        c.eq("toString", i, &a.to_java_string(), f[0]);
        if f[6] != "-" {
            c.eq("toPlainString", i, &a.to_plain_string(), f[6]);
        }
        c.eq("toEngineeringString", i, &a.to_engineering_string(), f[7]);
        c.eq("hashCode", i, &a.java_hash_code().to_string(), f[8]);
        c.eq("precision", i, &a.precision().to_string(), f[9]);
        c.eq("signum", i, &a.signum().to_string(), f[10]);
        c.eq("round", i, &bd(a.round(&m)), f[11]);
        c.eq("setScale", i, &bd(a.set_scale(k, r)), f[12]);
        c.eq("stripTrailingZeros", i, &bd(a.try_strip_trailing_zeros()), f[13]);
        if !f[13].starts_with("EX:") {
            c.eq("stripTrailingZeros (infallible)", i, &a.strip_trailing_zeros().to_java_string(), f[13]);
        }
        c.eq("longValueExact", i, &res(a.long_value_exact()), f[14]);
        c.eq("intValueExact", i, &res(a.int_value_exact()), f[15]);
        c.eq("longValue", i, &a.long_value().to_string(), f[16]);
        c.eq("intValue", i, &a.int_value().to_string(), f[17]);
        c.eq("doubleValue", i, &format!("{:x}", a.double_value().to_bits()), f[18]);
        c.eq("floatValue", i, &format!("{:x}", a.float_value().to_bits()), f[19]);
        c.eq("toBigInteger", i, &res(a.try_to_big_integer()), f[20]);
        if !f[20].starts_with("EX:") {
            c.eq("toBigInteger (infallible)", i, &a.to_big_integer().to_string(), f[20]);
        }
        c.eq("toBigIntegerExact", i, &res(a.to_big_integer_exact()), f[21]);
        if f[22] != "-" {
            c.eq("pow", &format!("{i}^{pw}"), &bd(a.pow(pw)), f[22]);
        }
        let pmc = MathContext { precision: f[23].parse().unwrap(), rounding_mode: m.rounding_mode };
        c.eq("pow(mc)", &format!("{i}^{pw} {pmc}"), &bd(a.pow_mc(pw, &pmc)), f[24]);
        c.eq("negate", i, &a.negate().to_java_string(), f[25]);
        c.eq("abs", i, &a.abs().to_java_string(), f[26]);
        c.eq("movePointLeft", i, &bd(a.move_point_left(k)), f[27]);
        c.eq("movePointRight", i, &bd(a.move_point_right(k)), f[28]);
        c.eq("ulp", i, &a.ulp().to_java_string(), f[29]);
        c.eq("MathContext.toString", i, &m.to_string(), f[30]);
        // equals/hashCode/compareTo consistency with a re-parse of the canonical string.
        let again = parse_bd(&a.to_java_string());
        c.eq("equals(self)", i, &a.java_equals(&again).to_string(), "true");
    }
    c.finish();
}

#[test]
fn big_decimal_binary() {
    let mut c = Check::new("bd_binary");
    for l in lines("bd_binary.txt") {
        let f: Vec<&str> = l.split('\t').collect();
        let a = parse_bd(f[0]);
        let b = parse_bd(f[1]);
        let m = mc(f[2], f[3]);
        let k: i32 = f[4].parse().unwrap();
        let r = rm(f[5]);
        let i = format!("{} {} {} k={} {}", f[0], f[1], m, k, f[5]);
        let i = i.as_str();
        if f[6] != "-" {
            c.eq("add", i, &bd(a.try_add(&b)), f[6]);
            if !f[6].starts_with("EX:") {
                c.eq("add (infallible)", i, &a.add(&b).to_java_string(), f[6]);
            }
        }
        if f[7] != "-" {
            c.eq("subtract", i, &bd(a.try_subtract(&b)), f[7]);
            if !f[7].starts_with("EX:") {
                c.eq("subtract (infallible)", i, &a.subtract(&b).to_java_string(), f[7]);
            }
        }
        c.eq("multiply", i, &bd(a.try_multiply(&b)), f[8]);
        if !f[8].starts_with("EX:") {
            c.eq("multiply (infallible)", i, &a.multiply(&b).to_java_string(), f[8]);
        }
        if f[9] != "-" {
            c.eq("add(mc)", i, &bd(a.add_mc(&b, &m)), f[9]);
        }
        if f[10] != "-" {
            c.eq("subtract(mc)", i, &bd(a.subtract_mc(&b, &m)), f[10]);
        }
        c.eq("multiply(mc)", i, &bd(a.multiply_mc(&b, &m)), f[11]);
        c.eq("divide(mc)", i, &bd(a.divide_mc(&b, &m)), f[12]);
        c.eq("divide", i, &bd(a.divide(&b)), f[13]);
        c.eq("divide(scale, rm)", i, &bd(a.divide_scale(&b, k, r)), f[14]);
        c.eq("divideToIntegralValue(mc)", i, &bd(a.divide_to_integral_value(&b, &m)), f[15]);
        c.eq("remainder(mc)", i, &bd(a.remainder_mc(&b, &m)), f[16]);
        c.eq("remainder", i, &bd(a.remainder(&b)), f[17]);
        c.eq(
            "divideToIntegralValue",
            i,
            &bd(a.divide_to_integral_value(&b, &MathContext::UNLIMITED)),
            f[18],
        );
        c.eq("compareTo", i, &(a.compare_to(&b) as i32).to_string(), f[19]);
        c.eq("equals", i, &a.java_equals(&b).to_string(), f[20]);
        c.eq("max", i, &a.max(&b).to_java_string(), f[21]);
        c.eq("min", i, &a.min(&b).to_java_string(), f[22]);
    }
    c.finish();
}

#[test]
fn fixed_api_facts() {
    // Constants and names that the fixtures only touch indirectly.
    assert_eq!(MathContext::DECIMAL128.to_string(), "precision=34 roundingMode=HALF_EVEN");
    assert_eq!(MathContext::DECIMAL64.to_string(), "precision=16 roundingMode=HALF_EVEN");
    assert_eq!(MathContext::DECIMAL32.to_string(), "precision=7 roundingMode=HALF_EVEN");
    assert_eq!(MathContext::UNLIMITED.to_string(), "precision=0 roundingMode=HALF_UP");
    for (m, n) in [
        (RoundingMode::Up, "UP"),
        (RoundingMode::Down, "DOWN"),
        (RoundingMode::Ceiling, "CEILING"),
        (RoundingMode::Floor, "FLOOR"),
        (RoundingMode::HalfUp, "HALF_UP"),
        (RoundingMode::HalfDown, "HALF_DOWN"),
        (RoundingMode::HalfEven, "HALF_EVEN"),
        (RoundingMode::Unnecessary, "UNNECESSARY"),
    ] {
        assert_eq!(m.name(), n);
        assert_eq!(RoundingMode::value_of(n), Some(m));
    }
    assert_eq!(RoundingMode::value_of("half_up"), None);
    assert_eq!(BigDecimal::zero().to_java_string(), "0");
    assert_eq!(BigDecimal::one().to_java_string(), "1");
    assert_eq!(BigDecimal::ten().to_java_string(), "10");
    assert_eq!(BigDecimal::from_i64(i64::MIN).to_java_string(), "-9223372036854775808");
    assert_eq!(BigDecimal::from_bigint(&BigInt::from(-5)).to_java_string(), "-5");
    let x = BigDecimal::new(BigInt::from(12345), 2);
    assert_eq!(x.unscaled_value(), &BigInt::from(12345));
    assert_eq!(x.to_java_string(), "123.45");
}
