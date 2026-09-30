// port of: java.lang.Double, java.lang.Float, java.lang.Integer, java.lang.Long, java.lang.Short, java.lang.Byte, java.lang.Character, java.math.BigInteger, jdk.internal.math.DoubleToDecimal, jdk.internal.math.FloatToDecimal, jdk.internal.math.FloatingDecimal
//! Java number <-> text conversions and hash codes, as implemented by JDK 25.
//!
//! Strings are processed as UTF-16 code units (like Java's `charAt`); error messages that echo a
//! string containing a lone surrogate use U+FFFD for it.
use num_bigint::{BigInt, Sign};
use num_traits::Zero;

/// `java.lang.NumberFormatException`; the payload is `getMessage()`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NumberFormatException(pub String);

fn nfe<T>(m: impl Into<String>) -> Result<T, NumberFormatException> {
    Err(NumberFormatException(m.into()))
}

// ------------------------------------------------------------------------------------------
// Double.toString / Float.toString (Raffaello Giulietti's Schubfach, JDK 19+)

fn mul_high(a: i64, b: i64) -> i64 {
    ((a as i128 * b as i128) >> 64) as i64
}

fn flog10pow2(e: i32) -> i32 {
    ((e as i64 * 661_971_961_083) >> 41) as i32
}

fn flog10_three_quarters_pow2(e: i32) -> i32 {
    ((e as i64 * 661_971_961_083 - 274_743_187_321) >> 41) as i32
}

fn flog2pow10(e: i32) -> i32 {
    ((e as i64 * 913_124_641_741) >> 38) as i32
}

fn g1(k: i32) -> i64 {
    G[(k + 324) as usize].0
}

fn g0(k: i32) -> i64 {
    G[(k + 324) as usize].1
}

const MASK_63: i64 = i64::MAX;

fn rop_d(g1: i64, g0: i64, cp: i64) -> i64 {
    let x1 = mul_high(g0, cp);
    let y0 = g1.wrapping_mul(cp);
    let y1 = mul_high(g1, cp);
    let z = ((y0 as u64 >> 1) as i64).wrapping_add(x1);
    let vbp = y1.wrapping_add((z as u64 >> 63) as i64);
    vbp | ((z & MASK_63).wrapping_add(MASK_63) as u64 >> 63) as i64
}

/// Shortest decimal `f * 10^e` for a finite positive double `c * 2^q` (DoubleToDecimal.toDecimal).
fn to_decimal_d(q: i32, c: i64, dk: i32) -> (i64, i32) {
    const C_MIN: i64 = 1 << 52;
    const Q_MIN: i32 = -1074;
    let out = c & 1;
    let cb = c << 2;
    let cbr = cb + 2;
    let (cbl, k) = if c != C_MIN || q == Q_MIN {
        (cb - 2, flog10pow2(q))
    } else {
        (cb - 1, flog10_three_quarters_pow2(q))
    };
    let h = (q + flog2pow10(-k) + 2) as u32;
    let (g1, g0) = (g1(k), g0(k));
    let vb = rop_d(g1, g0, cb.wrapping_shl(h));
    let vbl = rop_d(g1, g0, cbl.wrapping_shl(h));
    let vbr = rop_d(g1, g0, cbr.wrapping_shl(h));
    let s = vb >> 2;
    if s >= 100 {
        let sp10 = 10 * mul_high(s, 115_292_150_460_684_698 << 4);
        let tp10 = sp10 + 10;
        let upin = vbl + out <= sp10 << 2;
        let wpin = (tp10 << 2) + out <= vbr;
        if upin != wpin {
            return (if upin { sp10 } else { tp10 }, k);
        }
    }
    let t = s + 1;
    let uin = vbl + out <= s << 2;
    let win = (t << 2) + out <= vbr;
    if uin != win {
        return (if uin { s } else { t }, k + dk);
    }
    let cmp = vb - ((s + t) << 1);
    (if cmp < 0 || cmp == 0 && (s & 1) == 0 { s } else { t }, k + dk)
}

fn rop_f(g: i64, cp: i64) -> i32 {
    const MASK_32: i64 = (1 << 32) - 1;
    let x1 = mul_high(g, cp);
    let vbp = (x1 as u64 >> 31) as i64;
    (vbp | ((x1 & MASK_32) + MASK_32) >> 32) as i32
}

/// FloatToDecimal.toDecimal.
fn to_decimal_f(q: i32, c: i32, dk: i32) -> (i64, i32) {
    const C_MIN: i32 = 1 << 23;
    const Q_MIN: i32 = -149;
    let out = c & 1;
    let cb = (c << 2) as i64;
    let cbr = cb + 2;
    let (cbl, k) = if c != C_MIN || q == Q_MIN {
        (cb - 2, flog10pow2(q))
    } else {
        (cb - 1, flog10_three_quarters_pow2(q))
    };
    let h = (q + flog2pow10(-k) + 33) as u32;
    let g = g1(k) + 1;
    let vb = rop_f(g, cb.wrapping_shl(h));
    let vbl = rop_f(g, cbl.wrapping_shl(h));
    let vbr = rop_f(g, cbr.wrapping_shl(h));
    let s = vb >> 2;
    if s >= 100 {
        let sp10 = 10 * ((s as i64 * 1_717_986_919) as u64 >> 34) as i32;
        let tp10 = sp10 + 10;
        let upin = vbl + out <= sp10 << 2;
        let wpin = (tp10 << 2) + out <= vbr;
        if upin != wpin {
            return (if upin { sp10 } else { tp10 } as i64, k);
        }
    }
    let t = s + 1;
    let uin = vbl + out <= s << 2;
    let win = (t << 2) + out <= vbr;
    if uin != win {
        return (if uin { s } else { t } as i64, k + dk);
    }
    let cmp = vb - ((s + t) << 1);
    (if cmp < 0 || cmp == 0 && (s & 1) == 0 { s } else { t } as i64, k + dk)
}

/// ToDecimal.toChars: renders `f * 10^e` (f > 0) in Java's Double/Float.toString layout.
fn to_chars(neg: bool, f: i64, e: i32) -> String {
    let digits = f.to_string();
    let len = digits.len() as i32;
    let e = e + len; // value = 0.d1d2... * 10^e
    let d = digits.trim_end_matches('0');
    let mut s = String::with_capacity(26);
    if neg {
        s.push('-');
    }
    if 0 < e && e <= 7 {
        let e = e as usize;
        if d.len() > e {
            s.push_str(&d[..e]);
            s.push('.');
            s.push_str(&d[e..]);
        } else {
            s.push_str(d);
            s.push_str(&"0".repeat(e - d.len()));
            s.push_str(".0");
        }
    } else if -3 < e && e <= 0 {
        s.push_str("0.");
        s.push_str(&"0".repeat((-e) as usize));
        s.push_str(d);
    } else {
        s.push_str(&d[..1]);
        s.push('.');
        s.push_str(if d.len() > 1 { &d[1..] } else { "0" });
        s.push('E');
        s.push_str(&(e - 1).to_string());
    }
    s
}

/// `java.lang.Double.toString(double)` (JDK 19+ shortest-uniquely-distinguishing algorithm).
pub fn double_to_string(v: f64) -> String {
    const P: i32 = 53;
    const Q_MIN: i32 = -1074;
    let bits = v.to_bits() as i64;
    let t = bits & ((1 << 52) - 1);
    let bq = ((bits as u64 >> 52) & 0x7ff) as i32;
    if bq == 0x7ff {
        return if t != 0 { "NaN" } else if bits > 0 { "Infinity" } else { "-Infinity" }.into();
    }
    let neg = bits < 0;
    let (f, e) = if bq != 0 {
        let mq = -Q_MIN + 1 - bq;
        let c = (1 << 52) | t;
        if 0 < mq && mq < P && (c >> mq) << mq == c {
            (c >> mq, 0)
        } else {
            to_decimal_d(-mq, c, 0)
        }
    } else if t != 0 {
        if t < 3 {
            to_decimal_d(Q_MIN, 10 * t, -1)
        } else {
            to_decimal_d(Q_MIN, t, 0)
        }
    } else {
        return if neg { "-0.0" } else { "0.0" }.into();
    };
    to_chars(neg, f, e)
}

/// `java.lang.Float.toString(float)` (JDK 19+).
pub fn float_to_string(v: f32) -> String {
    const P: i32 = 24;
    const Q_MIN: i32 = -149;
    let bits = v.to_bits() as i32;
    let t = bits & ((1 << 23) - 1);
    let bq = ((bits as u32 >> 23) & 0xff) as i32;
    if bq == 0xff {
        return if t != 0 { "NaN" } else if bits > 0 { "Infinity" } else { "-Infinity" }.into();
    }
    let neg = bits < 0;
    let (f, e) = if bq != 0 {
        let mq = -Q_MIN + 1 - bq;
        let c = (1 << 23) | t;
        if 0 < mq && mq < P && (c >> mq) << mq == c {
            ((c >> mq) as i64, 0)
        } else {
            to_decimal_f(-mq, c, 0)
        }
    } else if t != 0 {
        if t < 8 {
            to_decimal_f(Q_MIN, 10 * t, -1)
        } else {
            to_decimal_f(Q_MIN, t, 0)
        }
    } else {
        return if neg { "-0.0" } else { "0.0" }.into();
    };
    to_chars(neg, f, e)
}

// ------------------------------------------------------------------------------------------
// Double.parseDouble / Float.parseFloat (FloatingDecimal.readJavaFormatString, JDK 25)

/// Binary format parameters, as FloatingDecimal's BINARY_32_IX / BINARY_64_IX tables.
struct Fmt {
    p: i32,
    q_min: i32,
    e_min: i32,
    e_max: i32,
    ep_min: i32,
    ep_max: i32,
    hex_count: i32,
}

const F64: Fmt = Fmt { p: 53, q_min: -1074, e_min: -1022, e_max: 1023, ep_min: -324, ep_max: 310, hex_count: 15 };
const F32: Fmt = Fmt { p: 24, q_min: -149, e_min: -126, e_max: 127, ep_min: -46, ep_max: 40, hex_count: 8 };

enum Scanned {
    Zero(bool),
    Inf(bool),
    NaN,
    /// Hexadecimal input, already rounded: sign, q, c (value = c * 2^q).
    Hex(bool, i32, i64),
    /// Decimal input: sign and the text `0.<digits>e<e>` for a correctly rounding parser.
    Dec(String),
}

fn check(u: &[u16], ok: bool) -> Result<(), NumberFormatException> {
    if ok {
        return Ok(());
    }
    let s = if u.len() > 1000 {
        let mut v = u[..497].to_vec();
        v.extend(" ... ".encode_utf16());
        v.extend_from_slice(&u[u.len() - 498..]);
        String::from_utf16_lossy(&v)
    } else {
        String::from_utf16_lossy(u)
    };
    nfe(format!("For input string: \"{s}\""))
}

fn skip_ws(u: &[u16], mut i: usize) -> usize {
    while i < u.len() && u[i] <= b' ' as u16 {
        i += 1;
    }
    i
}

fn is_fp_digit(ch: u16, is_dec: bool) -> bool {
    (b'0' as u16..=b'9' as u16).contains(&ch) || !is_dec && (b'a' as u16..=b'f' as u16).contains(&(ch | 0x20))
}

fn read_java_format_string(s: &str, ix: &Fmt) -> Result<Scanned, NumberFormatException> {
    let u: Vec<u16> = s.encode_utf16().collect();
    let len = u.len();
    let c = |ch: u8| ch as u16;
    let mut i = skip_ws(&u, 0);
    if i == len {
        return nfe("empty String");
    }
    let mut neg = false;
    if u[i] == c(b'-') || u[i] == c(b'+') {
        neg = u[i] == c(b'-');
        i += 1;
    }
    let mut is_dec = true;
    if i < len {
        let symbolic = |sub: &str| -> Result<(), NumberFormatException> {
            let sub: Vec<u16> = sub.encode_utf16().collect();
            let high = i + sub.len();
            check(&u, high <= len && u[i..high] == sub[..] && skip_ws(&u, high) == len)
        };
        if u[i] == c(b'I') {
            symbolic("Infinity")?;
            return Ok(Scanned::Inf(neg));
        }
        if u[i] == c(b'N') {
            symbolic("NaN")?;
            return Ok(Scanned::NaN);
        }
        if u[i] == c(b'0') && i + 1 < len && u[i + 1] | 0x20 == c(b'x') {
            is_dec = false;
            i += 2;
        }
    }
    let mut pt = 0usize; // index after point, 0 iff absent
    let start = i;
    let point = |pt: usize, i: usize| if pt != 0 { nfe("multiple points") } else { Ok(i) };
    while i < len && (u[i] == c(b'0') || u[i] == c(b'.')) {
        i += 1;
        if u[i - 1] == c(b'.') {
            pt = point(pt, i)?;
        }
    }
    let lz = i;
    let mut tnz = 0usize;
    while i < len && (is_fp_digit(u[i], is_dec) || u[i] == c(b'.')) {
        i += 1;
        if u[i - 1] == c(b'.') {
            pt = point(pt, i)?;
        } else if u[i - 1] != c(b'0') {
            tnz = i;
        }
    }
    check(&u, i - start > usize::from(pt != 0))?;
    let stop = i;
    let mut ep: i64 = 0;
    let mut has_exp = false;
    if i < len && (u[i] | 0x20 == c(b'e') && is_dec || u[i] | 0x20 == c(b'p') && !is_dec) {
        i += 1;
        let mut esign = b' ';
        if i < len && (u[i] == c(b'-') || u[i] == c(b'+')) {
            esign = u[i] as u8;
            i += 1;
        }
        while i < len && is_fp_digit(u[i], true) {
            ep = if ep < 1_000_000_000 { 10 * ep + (u[i] - c(b'0')) as i64 } else { 10_000_000_000 };
            i += 1;
        }
        check(&u, i - stop >= 3 || i - stop == 2 && esign == b' ')?;
        if esign == b'-' {
            ep = -ep;
        }
        has_exp = true;
    }
    check(&u, is_dec || has_exp)?;
    if i < len && (u[i] | 0x20 == c(b'f') || u[i] | 0x20 == c(b'd')) {
        i += 1;
    }
    check(&u, skip_ws(&u, i) == len)?;
    if tnz == 0 {
        return Ok(Scanned::Zero(neg));
    }
    let emult: i64 = if is_dec { 1 } else { 4 };
    let mut n = (tnz - lz) as i64;
    if pt == 0 {
        ep += emult * (stop - tnz) as i64;
    } else {
        ep += emult * (pt as i64 - tnz as i64);
        if pt > tnz {
            ep -= emult;
        } else if lz < pt {
            n -= 1;
        }
    }
    // Significant digits f_1 ... f_n in u[lz..tnz], skipping the point.
    let sig = u[lz..tnz].iter().copied().filter(|&ch| ch != c(b'.'));
    if !is_dec {
        let le = n.min(ix.hex_count as i64);
        let mut cc: i64 = 0;
        for ch in sig.take(le as usize) {
            let d = if ch <= c(b'9') { ch - c(b'0') } else { (ch | 0x20) - (c(b'a') - 10) };
            cc = cc << 4 | d as i64;
        }
        if n > le {
            cc |= 1;
            ep += 4 * (n - le);
        }
        let bl = 64 - cc.leading_zeros() as i64;
        if ep < ix.q_min as i64 - bl {
            return Ok(Scanned::Zero(neg));
        }
        if ep > ix.e_max as i64 - bl + 1 {
            return Ok(Scanned::Inf(neg));
        }
        let mut q = ep as i32;
        let bl = bl as i32;
        let shr;
        if q > ix.e_min - bl {
            shr = bl - ix.p;
            q += shr;
        } else {
            shr = ix.q_min - q;
            q = ix.q_min;
        }
        if shr > 0 {
            let thr = 1i64 << shr;
            let tail = (cc & (thr - 1)) << 1;
            cc >>= shr;
            if tail > thr || tail == thr && (cc & 1) != 0 {
                cc += 1;
                if cc >= 1 << ix.p {
                    cc >>= 1;
                    q += 1;
                }
            }
        } else {
            cc <<= -shr;
        }
        return Ok(Scanned::Hex(neg, q, cc));
    }
    let e = (ep + n).clamp(ix.ep_min as i64, ix.ep_max as i64) as i32;
    if e == ix.ep_min {
        return Ok(Scanned::Zero(neg));
    }
    if e == ix.ep_max {
        return Ok(Scanned::Inf(neg));
    }
    let ql = (flog2pow10(e - 1) - (ix.p - 1)).max(ix.q_min);
    let np = (e + (2 - ql).max(1)) as i64;
    let mut text = String::from(if neg { "-0." } else { "0." });
    if n >= np {
        text.extend(sig.take(np as usize - 1).map(|ch| ch as u8 as char));
        text.push('3'); // any non-zero sticky digit
    } else {
        text.extend(sig.map(|ch| ch as u8 as char));
    }
    text.push('e');
    text.push_str(&e.to_string());
    Ok(Scanned::Dec(text))
}

/// Assembles `c * 2^q` into IEEE bits (FloatingDecimal.buildDouble/buildFloat).
fn build_bits(ix: &Fmt, neg: bool, q: i32, c: i64) -> u64 {
    let width = if ix.p == 53 { 64 } else { 32 };
    let bias = ix.e_max as i64;
    let be = if c < 1 << (ix.p - 1) { 0 } else { q as i64 + (bias - 1) + ix.p as i64 };
    (u64::from(neg) << (width - 1)) | (be as u64) << (ix.p - 1) | (c as u64 & ((1 << (ix.p - 1)) - 1))
}

/// `java.lang.Double.parseDouble(String)`.
pub fn parse_double(s: &str) -> Result<f64, NumberFormatException> {
    Ok(match read_java_format_string(s, &F64)? {
        Scanned::Zero(neg) => if neg { -0.0 } else { 0.0 },
        Scanned::Inf(neg) => if neg { f64::NEG_INFINITY } else { f64::INFINITY },
        Scanned::NaN => f64::NAN,
        Scanned::Hex(neg, q, c) => f64::from_bits(build_bits(&F64, neg, q, c)),
        // ponytail: core's parser is correctly rounded, as is JDK 25's; the digits were validated above.
        Scanned::Dec(t) => t.parse().unwrap_or(f64::NAN),
    })
}

/// `java.lang.Float.parseFloat(String)` (rounds directly to float, no double rounding).
pub fn parse_float(s: &str) -> Result<f32, NumberFormatException> {
    Ok(match read_java_format_string(s, &F32)? {
        Scanned::Zero(neg) => if neg { -0.0 } else { 0.0 },
        Scanned::Inf(neg) => if neg { f32::NEG_INFINITY } else { f32::INFINITY },
        Scanned::NaN => f32::NAN,
        Scanned::Hex(neg, q, c) => f32::from_bits(build_bits(&F32, neg, q, c) as u32),
        Scanned::Dec(t) => t.parse().unwrap_or(f32::NAN),
    })
}

// ------------------------------------------------------------------------------------------
// Character.digit, Integer/Long/Short/Byte.parseXxx, new BigInteger(String, int)

/// `java.lang.Character.digit(char, int)`.
pub fn char_digit(c: u16, radix: u32) -> i32 {
    if !(2..=36).contains(&radix) {
        return -1;
    }
    let i = DIGIT_RUNS.partition_point(|&(_, end, _)| end < c);
    match DIGIT_RUNS.get(i) {
        Some(&(start, _, v0)) if start <= c => {
            let v = v0 as i32 + (c - start) as i32;
            if v < radix as i32 { v } else { -1 }
        }
        _ => -1,
    }
}

fn for_input_string<T>(s: &str, radix: u32) -> Result<T, NumberFormatException> {
    let suffix = if radix == 10 { String::new() } else { format!(" under radix {radix}") };
    nfe(format!("For input string: \"{s}\"{suffix}"))
}

fn check_radix(radix: u32) -> Result<(), NumberFormatException> {
    if radix < 2 {
        return nfe(format!("radix {radix} less than Character.MIN_RADIX"));
    }
    if radix > 36 {
        return nfe(format!("radix {radix} greater than Character.MAX_RADIX"));
    }
    Ok(())
}

/// Integer.parseInt / Long.parseLong on UTF-16 units, parameterised by the type's MIN_VALUE.
fn parse_signed(u: &[u16], radix: u32, min: i64) -> Option<i64> {
    let len = u.len();
    let first = *u.first()?;
    let r = radix as i64;
    let mut digit: i64 = !0xFF;
    let mut i = 1;
    if first != b'-' as u16 && first != b'+' as u16 {
        digit = char_digit(first, radix) as i64;
    }
    if digit >= 0 || digit == !0xFF && len > 1 {
        let limit = if first != b'-' as u16 { min + 1 } else { min };
        let multmin = limit / r;
        let mut result = -(digit & 0xFF);
        let mut in_range = true;
        while i < len {
            digit = char_digit(u[i], radix) as i64;
            i += 1;
            if digit < 0 {
                break;
            }
            in_range = result > multmin || result == multmin && digit <= r * multmin - limit;
            if !in_range {
                break;
            }
            result = r * result - digit;
        }
        if in_range && i == len && digit >= 0 {
            return Some(if first != b'-' as u16 { -result } else { result });
        }
    }
    None
}

fn parse_int_units(u: &[u16], radix: u32) -> Option<i32> {
    parse_signed(u, radix, i32::MIN as i64).map(|v| v as i32)
}

/// `java.lang.Integer.parseInt(String, int)`.
pub fn parse_int(s: &str, radix: u32) -> Result<i32, NumberFormatException> {
    check_radix(radix)?;
    let u: Vec<u16> = s.encode_utf16().collect();
    parse_int_units(&u, radix).map_or_else(|| for_input_string(s, radix), Ok)
}

/// `java.lang.Long.parseLong(String, int)`.
pub fn parse_long(s: &str, radix: u32) -> Result<i64, NumberFormatException> {
    check_radix(radix)?;
    let u: Vec<u16> = s.encode_utf16().collect();
    parse_signed(&u, radix, i64::MIN).map_or_else(|| for_input_string(s, radix), Ok)
}

fn parse_narrow(s: &str, radix: u32, min: i32, max: i32) -> Result<i32, NumberFormatException> {
    let i = parse_int(s, radix)?;
    if i < min || i > max {
        return nfe(format!("Value out of range. Value:\"{s}\" Radix:{radix}"));
    }
    Ok(i)
}

/// `java.lang.Short.parseShort(String, int)`.
pub fn parse_short(s: &str, radix: u32) -> Result<i16, NumberFormatException> {
    parse_narrow(s, radix, i16::MIN as i32, i16::MAX as i32).map(|v| v as i16)
}

/// `java.lang.Byte.parseByte(String, int)`.
pub fn parse_byte(s: &str, radix: u32) -> Result<i8, NumberFormatException> {
    parse_narrow(s, radix, i8::MIN as i32, i8::MAX as i32).map(|v| v as i8)
}

/// BigInteger.digitsPerInt.
const DIGITS_PER_INT: [usize; 37] = [
    0, 0, 30, 19, 15, 13, 11, 11, 10, 9, 9, 8, 8, 8, 8, 7, 7, 7, 7, 7, 7, 7, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6,
    6, 5,
];

/// `new java.math.BigInteger(String, int)`.
pub fn parse_big_integer(s: &str, radix: u32) -> Result<BigInt, NumberFormatException> {
    if !(2..=36).contains(&radix) {
        return nfe("Radix out of range");
    }
    let u: Vec<u16> = s.encode_utf16().collect();
    let len = u.len();
    if len == 0 {
        return nfe("Zero length BigInteger");
    }
    let last = |ch: u8| u.iter().rposition(|&x| x == ch as u16);
    let (minus, plus) = (last(b'-'), last(b'+'));
    let mut cursor = 0;
    let mut sign = Sign::Plus;
    if let Some(i1) = minus {
        if i1 != 0 || plus.is_some() {
            return nfe("Illegal embedded sign character");
        }
        sign = Sign::Minus;
        cursor = 1;
    } else if let Some(i2) = plus {
        if i2 != 0 {
            return nfe("Illegal embedded sign character");
        }
        cursor = 1;
    }
    if cursor == len {
        return nfe("Zero length BigInteger");
    }
    while cursor < len && char_digit(u[cursor], radix) == 0 {
        cursor += 1;
    }
    if cursor == len {
        return Ok(BigInt::zero());
    }
    // ponytail: Java's "BigInteger would overflow supported range" needs a > 600M-digit string.
    let per = DIGITS_PER_INT[radix as usize];
    let first = match (len - cursor) % per {
        0 => per,
        n => n,
    };
    let mut at = cursor;
    let mut group_len = first;
    while at < len {
        let group = &u[at..at + group_len];
        if parse_int_units(group, radix).is_none() {
            return for_input_string(&String::from_utf16_lossy(group), radix);
        }
        at += group_len;
        group_len = per;
    }
    let digits: Vec<u8> = u[cursor..].iter().map(|&ch| char_digit(ch, radix) as u8).collect();
    Ok(BigInt::from_radix_be(sign, &digits, radix).unwrap_or_default())
}

/// `java.math.BigInteger.toString(int)` (radix outside 2..=36 means 10).
pub fn big_integer_to_string(b: &BigInt, radix: u32) -> String {
    b.to_str_radix(if (2..=36).contains(&radix) { radix } else { 10 })
}

// ------------------------------------------------------------------------------------------
// hashCode

/// `java.math.BigInteger.hashCode()`.
pub fn big_integer_hash_code(b: &BigInt) -> i32 {
    let mut h: i32 = 0;
    for w in b.magnitude().to_u32_digits().iter().rev() {
        h = h.wrapping_mul(31).wrapping_add(*w as i32);
    }
    match b.sign() {
        Sign::Minus => h.wrapping_neg(),
        Sign::NoSign => 0,
        Sign::Plus => h,
    }
}

/// `java.lang.Long.hashCode(long)`.
pub fn long_hash_code(l: i64) -> i32 {
    (l ^ ((l as u64) >> 32) as i64) as i32
}

/// `java.lang.Double.hashCode(double)` (doubleToLongBits: NaN is canonical).
pub fn double_hash_code(d: f64) -> i32 {
    long_hash_code(if d.is_nan() { 0x7ff8_0000_0000_0000 } else { d.to_bits() as i64 })
}

/// `java.lang.Float.hashCode(float)` (floatToIntBits: NaN is canonical).
pub fn float_hash_code(f: f32) -> i32 {
    if f.is_nan() { 0x7fc0_0000 } else { f.to_bits() as i32 }
}

/// BMP runs `(first, last, digit value of first)` with `Character.digit(c, 36) != -1`
/// (dumped from Corretto 25 by tools/javagen/NumberGen.java, char_digit.txt).
#[rustfmt::skip]
static DIGIT_RUNS: [(u16, u16, u8); 41] = [
    (0x30, 0x39, 0),
    (0x41, 0x5a, 10),
    (0x61, 0x7a, 10),
    (0x660, 0x669, 0),
    (0x6f0, 0x6f9, 0),
    (0x7c0, 0x7c9, 0),
    (0x966, 0x96f, 0),
    (0x9e6, 0x9ef, 0),
    (0xa66, 0xa6f, 0),
    (0xae6, 0xaef, 0),
    (0xb66, 0xb6f, 0),
    (0xbe6, 0xbef, 0),
    (0xc66, 0xc6f, 0),
    (0xce6, 0xcef, 0),
    (0xd66, 0xd6f, 0),
    (0xde6, 0xdef, 0),
    (0xe50, 0xe59, 0),
    (0xed0, 0xed9, 0),
    (0xf20, 0xf29, 0),
    (0x1040, 0x1049, 0),
    (0x1090, 0x1099, 0),
    (0x17e0, 0x17e9, 0),
    (0x1810, 0x1819, 0),
    (0x1946, 0x194f, 0),
    (0x19d0, 0x19d9, 0),
    (0x1a80, 0x1a89, 0),
    (0x1a90, 0x1a99, 0),
    (0x1b50, 0x1b59, 0),
    (0x1bb0, 0x1bb9, 0),
    (0x1c40, 0x1c49, 0),
    (0x1c50, 0x1c59, 0),
    (0xa620, 0xa629, 0),
    (0xa8d0, 0xa8d9, 0),
    (0xa900, 0xa909, 0),
    (0xa9d0, 0xa9d9, 0),
    (0xa9f0, 0xa9f9, 0),
    (0xaa50, 0xaa59, 0),
    (0xabf0, 0xabf9, 0),
    (0xff10, 0xff19, 0),
    (0xff21, 0xff3a, 10),
    (0xff41, 0xff5a, 10),
];

/// `jdk.internal.math.MathUtils.g`: pairs (g1, g0) for k = -324..=292.
#[rustfmt::skip]
static G: [(i64, i64); 617] = [
    (0x4f0c_edc9_5a71_8dd4, 0x5b01_e8b0_9aa0_d1b5), // -324
    (0x7e7b_160e_f71c_1621, 0x119c_a780_f767_b5ee), // -323
    (0x652f_44d8_c5b0_11b4, 0x0e16_ec67_2c52_f7f2), // -322
    (0x50f2_9d7a_37c0_0e29, 0x5812_56b8_f042_5ff5), // -321
    (0x40c2_1794_f966_71ba, 0x79a8_4560_c035_1991), // -320
    (0x679c_f287_f570_b5f7, 0x75da_089a_cd21_c281), // -319
    (0x52e3_f539_9126_f7f9, 0x44ae_6d48_a41b_0201), // -318
    (0x424f_f761_40eb_f994, 0x36f1_f106_e9af_34cd), // -317
    (0x6a19_8bce_ce46_5c20, 0x57e9_81a4_a918_547b), // -316
    (0x54e1_3ca5_71d1_e34d, 0x2cba_ce1d_5413_76c9), // -315
    (0x43e7_63b7_8e41_82a4, 0x23c8_a4e4_4342_c56e), // -314
    (0x6ca5_6c58_e39c_043a, 0x060d_d4a0_6b9e_08b0), // -313
    (0x56ea_bd13_e949_9cfb, 0x1e71_76e6_bc7e_6d59), // -312
    (0x4588_9743_2107_b0c8, 0x7ec1_2beb_c9fe_bde1), // -311
    (0x6f40_f205_01a5_e7a7, 0x7e01_dfdf_a997_9635), // -310
    (0x5900_c19d_9aeb_1fb9, 0x4b34_b319_5479_44f7), // -309
    (0x4733_ce17_af22_7fc7, 0x55c3_c27a_a9fa_9d93), // -308
    (0x71ec_7cf2_b1d0_cc72, 0x5606_03f7_765d_c8ea), // -307
    (0x5b23_9728_8e40_a38e, 0x7804_cff9_2b7e_3a55), // -306
    (0x48e9_45ba_0b66_e93f, 0x1337_0cc7_55fe_9511), // -305
    (0x74a8_6f90_123e_41fe, 0x51f1_ae0b_bcca_881b), // -304
    (0x5d53_8c73_41cb_67fe, 0x74c1_5809_63d5_39af), // -303
    (0x4aa9_3d29_016f_8665, 0x43cd_e007_8310_faf3), // -302
    (0x7775_2ea8_024c_0a3c, 0x0616_333f_381b_2b1e), // -301
    (0x5f90_f220_01d6_6e96, 0x3811_c298_f9af_55b1), // -300
    (0x4c73_f4e6_67de_bede, 0x600e_3547_2e25_de28), // -299
    (0x7a53_2170_a631_3164, 0x3349_eed8_49d6_303f), // -298
    (0x61dc_1ac0_84f4_2783, 0x42a1_8be0_3b11_c033), // -297
    (0x4e49_af00_6a5c_ec69, 0x1bb4_6fe6_95a7_ccf5), // -296
    (0x7d42_b19a_43c7_e0a8, 0x2c53_e63d_bc3f_ae55), // -295
    (0x6435_5ae1_cfd3_1a20, 0x2376_51ca_fcff_beaa), // -294
    (0x502a_af1b_0ca8_e1b3, 0x35f8_416f_30cc_9888), // -293
    (0x4022_25af_3d53_e7c2, 0x5e60_3458_f3d6_e06d), // -292
    (0x669d_0918_621f_d937, 0x4a33_86f4_b957_cd7b), // -291
    (0x5217_3a79_e819_7a92, 0x6e8f_9f2a_2ddf_d796), // -290
    (0x41ac_2ec7_ece1_2edb, 0x720c_7f54_f17f_dfab), // -289
    (0x6913_7e0c_ae35_17c6, 0x1ce0_cbbb_1bff_cc45), // -288
    (0x540f_980a_24f7_4638, 0x171a_3c95_afff_d69e), // -287
    (0x433f_acd4_ea5f_6b60, 0x127b_63aa_f333_1218), // -286
    (0x6b99_1487_dd65_7899, 0x6a5f_05de_51eb_5026), // -285
    (0x5614_106c_b11d_fa14, 0x5518_d17e_a7ef_7352), // -284
    (0x44dc_d9f0_8db1_94dd, 0x2a7a_4132_1ff2_c2a8), // -283
    (0x6e2e_2980_e2b5_bafb, 0x5d90_6850_331e_043f), // -282
    (0x5824_ee00_b55e_2f2f, 0x6473_86a6_8f4b_3699), // -281
    (0x4683_f19a_2ab1_bf59, 0x36c2_d21e_d908_f87b), // -280
    (0x70d3_1c29_dde9_3228, 0x579e_1cfe_280e_5a5d), // -279
    (0x5a42_7cee_4b20_f4ed, 0x2c7e_7d98_200b_7b7e), // -278
    (0x4835_30be_a280_c3f1, 0x09fe_cae0_19a2_c932), // -277
    (0x7388_4dfd_d0ce_064e, 0x4331_4499_c29e_0eb6), // -276
    (0x5c6d_0b31_73d8_050b, 0x4f5a_9d47_cee4_d891), // -275
    (0x49f0_d5c1_2979_9da2, 0x72ae_e439_7250_ad41), // -274
    (0x764e_22ce_a8c2_95d1, 0x377e_39f5_83b4_4868), // -273
    (0x5ea4_e8a5_53ce_de41, 0x12cb_6191_3629_d387), // -272
    (0x4bb7_2084_430b_e500, 0x756f_8140_f821_7605), // -271
    (0x7925_00d3_9e79_6e67, 0x6f18_cece_59cf_233c), // -270
    (0x60ea_670f_b1fa_beb9, 0x3f47_0bd8_47d8_e8fd), // -269
    (0x4d88_5272_f4c8_9894, 0x329f_3cad_0647_20ca), // -268
    (0x7c0d_50b7_ee0d_c0ed, 0x3765_2de1_a3a5_0143), // -267
    (0x633d_da2c_be71_6724, 0x2c50_f181_4fb7_3436), // -266
    (0x4f64_ae8a_31f4_5283, 0x3d0d_8e01_0c92_902b), // -265
    (0x7f07_7da9_e986_ea6b, 0x7b48_e334_e0ea_8045), // -264
    (0x659f_97bb_2138_bb89, 0x4907_1c2a_4d88_669d), // -263
    (0x514c_7962_80fa_2fa1, 0x20d2_7cee_a46d_1ee4), // -262
    (0x4109_fab5_33fb_594d, 0x670e_ca58_838a_7f1d), // -261
    (0x680f_f788_532b_c216, 0x0b4a_dd5a_6c10_cb62), // -260
    (0x533f_f939_dc23_01ab, 0x22a2_4aae_bcda_3c4e), // -259
    (0x4299_942e_49b5_9aef, 0x354e_a225_63e1_c9d8), // -258
    (0x6a8f_537d_42bc_2b18, 0x554a_9d08_9fcf_a95a), // -257
    (0x553f_75fd_cefc_ef46, 0x776e_e406_e63f_baae), // -256
    (0x4432_c4cb_0bfd_8c38, 0x5f8b_e99f_1e99_6225), // -255
    (0x6d1e_07ab_4662_79f4, 0x3279_75cb_6428_9d08), // -254
    (0x574b_3955_d1e8_6190, 0x2861_2b09_1ced_4a6d), // -253
    (0x45d5_c777_db20_4e0d, 0x06b4_226d_b0bd_d524), // -252
    (0x6fbc_7259_5e9a_167b, 0x2453_6a49_1ac9_5506), // -251
    (0x5963_8ead_e548_11fc, 0x1d0f_883a_7bd4_4405), // -250
    (0x4782_d88b_1dd3_4196, 0x4a72_d361_fca9_d004), // -249
    (0x726a_f411_c952_028a, 0x43ea_ebcf_faa9_4cd3), // -248
    (0x5b88_c341_6ddb_353b, 0x4fef_230c_c887_70a9), // -247
    (0x493a_35cd_f17c_2a96, 0x0cbf_4f3d_6d39_26ee), // -246
    (0x7529_efaf_e8c6_aa89, 0x6132_1862_485b_717c), // -245
    (0x5dbb_2626_53d2_2207, 0x675b_46b5_06af_8dfd), // -244
    (0x4afc_1e85_0fdb_4e6c, 0x52af_6bc4_0559_3e64), // -243
    (0x77f9_ca6e_7fc5_4a47, 0x377f_12d3_3bc1_fd6d), // -242
    (0x5ffb_0858_6637_6e9f, 0x45ff_4242_9634_cabd), // -241
    (0x4cc8_d379_eb5f_8bb2, 0x6b32_9b68_782a_3bcb), // -240
    (0x7ada_ebf6_4565_ac51, 0x2b84_2bda_59dd_2c77), // -239
    (0x6248_bcc5_0451_56a7, 0x3c69_bcae_ae4a_89f9), // -238
    (0x4ea0_9704_0374_4552, 0x6387_ca25_583b_a194), // -237
    (0x7dcd_be6c_d253_a21e, 0x05a6_103b_c05f_68ed), // -236
    (0x64a4_9857_0ea9_4e7e, 0x37b8_0cfc_99e5_ed8a), // -235
    (0x5083_ad12_7221_0b98, 0x2c93_3d96_e184_be08), // -234
    (0x4069_5741_f4e7_3c79, 0x7075_cadf_1ad0_9807), // -233
    (0x670e_f203_2171_fa5c, 0x4d89_4498_2ae7_59a4), // -232
    (0x5272_5b35_b45b_2eb0, 0x3e07_6a13_5585_e150), // -231
    (0x41f5_15c4_9048_f226, 0x64d2_bb42_aad1_810d), // -230
    (0x6988_22d4_1a0e_503e, 0x07b7_9204_4482_6815), // -229
    (0x546c_e8a9_ae71_d9cb, 0x1fc6_0e69_d068_5344), // -228
    (0x438a_53ba_f1f4_ae3c, 0x196b_3ebb_0d20_429d), // -227
    (0x6c10_85f7_e987_7d2d, 0x0f11_fdf8_1500_6a94), // -226
    (0x5673_9e5f_ee05_fdbd, 0x58db_3193_4400_5543), // -225
    (0x4529_4b7f_f19e_6497, 0x60af_5adc_3666_aa9c), // -224
    (0x6ea8_78cc_b5ca_3a8c, 0x344b_c493_8a3d_ddc7), // -223
    (0x5886_c70a_2b08_2ed6, 0x5d09_6a0f_a1cb_17d2), // -222
    (0x46d2_38d4_ef39_bf12, 0x173a_bb3f_b4a2_7975), // -221
    (0x7150_5aee_4b8f_981d, 0x0b91_2b99_2103_f588), // -220
    (0x5aa6_af25_093f_ace4, 0x0940_efad_b403_2ad3), // -219
    (0x4885_58ea_6dcc_8a50, 0x0767_2624_9002_88a9), // -218
    (0x7408_8e43_e2e0_dd4c, 0x723e_a36d_b337_410e), // -217
    (0x5cd3_a503_1be7_1770, 0x5b65_4f8a_f5c5_cda5), // -216
    (0x4a42_ea68_e31f_45f3, 0x62b7_72d5_916b_0aeb), // -215
    (0x76d1_770e_3832_0986, 0x0458_b7bc_1bde_77dd), // -214
    (0x5f0d_f8d8_2cf4_d46b, 0x1d13_c630_164b_9318), // -213
    (0x4c0b_2d79_bd90_a9ef, 0x30dc_9e8c_dea2_dc13), // -212
    (0x79ab_7bf5_fc1a_a97f, 0x0160_fdae_3104_9351), // -211
    (0x6155_fcc4_c9ae_edff, 0x1ab3_fe24_f403_a90e), // -210
    (0x4dde_63d0_a158_be65, 0x6229_981d_9002_eda5), // -209
    (0x7c97_061a_9bc1_30a2, 0x69dc_2695_b337_e2a1), // -208
    (0x63ac_04e2_1634_26e8, 0x54b0_1ede_28f9_821b), // -207
    (0x4fbc_d0b4_de90_1f20, 0x43c0_18b1_ba61_34e2), // -206
    (0x7f94_8121_6419_cb67, 0x1f99_c11c_5d68_549d), // -205
    (0x6610_674d_e9ae_3c52, 0x4c7b_00e3_7ded_107e), // -204
    (0x51a6_b90b_2158_3042, 0x09fc_00b5_fe57_4065), // -203
    (0x4152_2da2_8113_59ce, 0x3b30_0091_9845_cd1d), // -202
    (0x6883_7c37_34eb_c2e3, 0x784c_cdb5_c06f_ae95), // -201
    (0x539c_635f_5d89_68b6, 0x2d0a_3e2b_0059_5877), // -200
    (0x42e3_82b2_b13a_ba2b, 0x3da1_cb55_99e1_1393), // -199
    (0x6b05_9dea_b52a_c378, 0x629c_7888_f634_ec1e), // -198
    (0x559e_17ee_f755_692d, 0x3549_fa07_2b5d_89b1), // -197
    (0x447e_798b_f911_20f1, 0x1107_fb38_ef7e_07c1), // -196
    (0x6d97_28df_f4e8_34b5, 0x01a6_5ec1_7f30_0c68), // -195
    (0x57ac_20b3_2a53_5d5d, 0x4e1e_b234_65c0_09ed), // -194
    (0x4623_4d5c_21dc_4ab1, 0x24e5_5b5d_1e33_3b24), // -193
    (0x7038_7bc6_9c93_aab5, 0x216e_f894_fd1e_c506), // -192
    (0x59c6_c96b_b076_222a, 0x4df2_6077_30e5_6a6c), // -191
    (0x47d2_3abc_8d2b_4e88, 0x3e5b_805f_5a51_21f0), // -190
    (0x72e9_f794_1512_1740, 0x63c5_9a32_2a1b_697f), // -189
    (0x5bee_5fa9_aa74_df67, 0x0304_7b5b_54e2_bacc), // -188
    (0x498b_7fba_eec3_e5ec, 0x0269_fc49_10b5_623d), // -187
    (0x75ab_ff91_7e06_3cac, 0x6a43_2d41_b455_69fb), // -186
    (0x5e23_32da_cb38_308a, 0x21cf_5767_c377_87fc), // -185
    (0x4b4f_5be2_3c2c_f3a1, 0x67d9_12b9_692c_6cca), // -184
    (0x787e_f969_f9e1_85cf, 0x595b_5128_a847_1476), // -183
    (0x6065_9454_c7e7_9e3f, 0x6115_da86_ed05_a9f8), // -182
    (0x4d1e_1043_d31f_b1cc, 0x4dab_1538_bd9e_2193), // -181
    (0x7b63_4d39_51cc_4fad, 0x62ab_5527_95c9_cf52), // -180
    (0x62b5_d761_0e3d_0c8b, 0x0222_aa86_116e_3f75), // -179
    (0x4ef7_df80_d830_d6d5, 0x4e82_2204_dabe_992a), // -178
    (0x7e59_659a_f381_57bc, 0x1736_9cd4_9130_f510), // -177
    (0x6514_5148_c2cd_dfc9, 0x5f5e_e3dd_40f3_f740), // -176
    (0x50dd_0dd3_cf0b_196e, 0x1918_b64a_9a5c_c5cd), // -175
    (0x40b0_d7dc_a5a2_7abe, 0x4746_f83b_aeb0_9e3e), // -174
    (0x6781_5961_0903_f797, 0x253e_59f9_1780_fd2f), // -173
    (0x52cd_e11a_6d9c_c612, 0x50fe_ae60_df9a_6426), // -172
    (0x423e_4dae_be17_04db, 0x5a65_584d_7fae_b685), // -171
    (0x69fd_4917_968b_3af9, 0x10a2_26e2_65e4_573b), // -170
    (0x54ca_a0df_aba2_9594, 0x0d4e_8581_eb1d_1295), // -169
    (0x43d5_4d7f_bc82_1143, 0x243e_d134_bc17_4211), // -168
    (0x6c88_7bff_9403_4ed2, 0x06ca_e854_6025_3682), // -167
    (0x56d3_9666_1002_a574, 0x6bd5_86a9_e684_2b9b), // -166
    (0x4576_11eb_4002_1df7, 0x0977_9eee_5203_5616), // -165
    (0x6f23_4fde_ccd0_2ff1, 0x5bf2_97e3_b66b_bcef), // -164
    (0x58e9_0cb2_3d73_598e, 0x165b_acb6_2b89_63f3), // -163
    (0x4720_d6f4_fdf5_e13e, 0x4516_23c4_efa1_1cc2), // -162
    (0x71ce_24bb_2fef_ceca, 0x3b56_9fa1_7f68_2e03), // -161
    (0x5b0b_5095_bff3_0bd5, 0x15de_e61a_cc53_5803), // -160
    (0x48d5_da11_665c_0977, 0x2b18_b815_7042_accf), // -159
    (0x7489_5ce8_a3c6_758b, 0x5e8d_f355_806a_ae18), // -158
    (0x5d3a_b0ba_1c9e_c46f, 0x653e_5c44_66bb_be7a), // -157
    (0x4a95_5a2e_7d4b_d059, 0x3765_169d_1efc_9861), // -156
    (0x7755_5d17_2edf_b3c2, 0x256e_8a94_fe60_f3cf), // -155
    (0x5f77_7dac_257f_c301, 0x6abe_d543_feb3_f63f), // -154
    (0x4c5f_97bc_eacc_9c01, 0x3bcb_ddcf_fef6_5e99), // -153
    (0x7a32_8c61_77ad_c668, 0x5fac_9619_97f0_975b), // -152
    (0x61c2_09e7_92f1_6b86, 0x7fbd_44e1_465a_12af), // -151
    (0x4e34_d4b9_425a_bc6b, 0x7fca_9d81_0514_dbbf), // -150
    (0x7d21_545b_9d5d_fa46, 0x32dd_c8ce_6e87_c5ff), // -149
    (0x641a_a9e2_e44b_2e9e, 0x5be4_a0a5_2539_6b32), // -148
    (0x5015_54b5_836f_587e, 0x7cb6_e6ea_842d_ef5c), // -147
    (0x4011_1091_35f2_ad32, 0x3092_5255_368b_25e3), // -146
    (0x6681_b41b_8984_4850, 0x4db6_ea21_f0de_a304), // -145
    (0x5201_5ce2_d469_d373, 0x57c5_881b_2718_826a), // -144
    (0x419a_b0b5_76bb_0f8f, 0x5fd1_39af_527a_01ef), // -143
    (0x68f7_8122_5791_b27f, 0x4c81_f5e5_50c3_364a), // -142
    (0x53f9_341b_7941_5b99, 0x239b_2b1d_da35_c508), // -141
    (0x432d_c349_2dcd_e2e1, 0x02e2_88e4_ae91_6a6d), // -140
    (0x6b7c_6ba8_4949_6b01, 0x516a_74a1_174f_10ae), // -139
    (0x55fd_22ed_076d_ef34, 0x4121_f6e7_45d8_da25), // -138
    (0x44ca_8257_3924_bf5d, 0x1a81_9252_9e47_14eb), // -137
    (0x6e10_d08b_8ea1_322e, 0x5d9c_1d50_fd3e_87dd), // -136
    (0x580d_73a2_d880_f4f2, 0x17b0_1773_fdcb_9fe4), // -135
    (0x4671_294f_139a_5d8e, 0x4626_7929_97d6_1984), // -134
    (0x70b5_0ee4_ec2a_2f4a, 0x3d0a_5b75_bfbc_f59f), // -133
    (0x5a2a_7250_bcee_8c3b, 0x4a6e_af91_6630_c47f), // -132
    (0x4821_f50d_63f2_09c9, 0x21f2_260d_eb5a_36cc), // -131
    (0x7369_8815_6cb6_760e, 0x6983_7016_455d_247a), // -130
    (0x5c54_6cdd_f091_f80b, 0x6e02_c011_d117_5062), // -129
    (0x49dd_23e4_c074_c66f, 0x719b_ccdb_0dac_404e), // -128
    (0x762e_9fd4_6721_3d7f, 0x68f9_47c4_e2ad_33b0), // -127
    (0x5e8b_b310_5280_fdff, 0x6d94_396a_4ef0_f627), // -126
    (0x4ba2_f5a6_a867_3199, 0x3e10_2dee_a58d_91b9), // -125
    (0x7904_bc3d_da3e_b5c2, 0x3019_e317_6f48_e927), // -124
    (0x60d0_9697_e1cb_c49b, 0x4014_b5ac_5907_20ec), // -123
    (0x4d73_abac_b4a3_03af, 0x4cdd_5e23_7a6c_1a57), // -122
    (0x7bec_45e1_2104_d2b2, 0x47c8_969f_2a46_908a), // -121
    (0x6323_6b1a_80d0_a88e, 0x6ca0_787f_5505_406f), // -120
    (0x4f4f_88e2_00a6_ed3f, 0x0a19_f9ff_7737_66bf), // -119
    (0x7ee5_a7d0_010b_1531, 0x5cf6_5ccb_f1f2_3dfe), // -118
    (0x6584_8640_00d5_aa8e, 0x172b_7d6f_f4c1_cb32), // -117
    (0x5136_d1cc_cd77_bba4, 0x78ef_978c_c3ce_3c28), // -116
    (0x40f8_a7d7_0ac6_2fb7, 0x13f2_dfa3_cfd8_3020), // -115
    (0x67f4_3fbe_77a3_7f8b, 0x3984_9906_1959_e699), // -114
    (0x5329_cc98_5fb5_ffa2, 0x6136_e0d1_ade1_8548), // -113
    (0x4287_d6e0_4c91_994f, 0x00f8_b3da_f181_376d), // -112
    (0x6a72_f166_e0e8_f54b, 0x1b27_862b_1c01_f247), // -111
    (0x5528_c11f_1a53_f76f, 0x2f52_d1bc_1667_f506), // -110
    (0x4420_9a7f_4843_2c59, 0x0c42_4163_451f_f738), // -109
    (0x6d00_f732_0d38_46f4, 0x7a03_9bd2_0833_2526), // -108
    (0x5733_f8f4_d760_38c3, 0x7b36_1641_a028_ea85), // -107
    (0x45c3_2d90_ac4c_fa36, 0x2f5e_7834_8020_bb9e), // -106
    (0x6f9e_af4d_e07b_29f0, 0x4bca_59ed_99cd_f8fc), // -105
    (0x594b_bf71_8062_87f3, 0x563b_7b24_7b0b_2d96), // -104
    (0x476f_cc5a_cd1b_9ff6, 0x11c9_2f50_626f_57ac), // -103
    (0x724c_7a2a_e1c5_ccbd, 0x02db_7ee7_03e5_5912), // -102
    (0x5b70_61bb_e7d1_7097, 0x1be2_cbec_031d_e0dc), // -101
    (0x4926_b496_530d_f3ac, 0x164f_0989_9c17_e716), // -100
    (0x750a_ba8a_1e7c_b913, 0x3d4b_4275_c68c_a4f0), // -99
    (0x5da2_2ed4_e530_940f, 0x4aa2_9b91_6ba3_b726), // -98
    (0x4ae8_2577_1dc0_7672, 0x6ee8_7c74_561c_9285), // -97
    (0x77d9_d58b_62cd_8a51, 0x3173_fa53_bcfa_8408), // -96
    (0x5fe1_77a2_b571_3b74, 0x278f_fb76_30c8_69a0), // -95
    (0x4cb4_5fb5_5df4_2f90, 0x1fa6_62c4_f3d3_87b3), // -94
    (0x7aba_32bb_c986_b280, 0x32a3_d13b_1fb8_d91f), // -93
    (0x622e_8efc_a138_8ecd, 0x0ee9_742f_4c93_e0e6), // -92
    (0x4e8b_a596_e760_723d, 0x58ba_c359_0a0f_e71e), // -91
    (0x7dac_3c24_a567_1d2f, 0x412a_d228_1019_71c9), // -90
    (0x6489_c9b6_eab8_e426, 0x00ef_0e86_7347_8e3b), // -89
    (0x506e_3af8_bbc7_1ceb, 0x1a58_d86b_8f6c_71c9), // -88
    (0x4058_2f2d_6305_b0bc, 0x1513_e056_0c56_c16e), // -87
    (0x66f3_7eaf_04d5_e793, 0x3b53_0089_ad57_9be2), // -86
    (0x525c_6558_d0ab_1fa9, 0x15dc_006e_2446_164f), // -85
    (0x41e3_8447_0d55_b2ed, 0x5e49_99f1_b69e_783f), // -84
    (0x696c_06d8_1555_eb15, 0x7d42_8fe9_2430_c065), // -83
    (0x5456_6be0_1111_88de, 0x3102_0cba_835a_3384), // -82
    (0x4378_564c_da74_6d7e, 0x5a68_0a2e_cf7b_5c69), // -81
    (0x6bf3_bd47_c3ed_7bfd, 0x770c_dd17_b25e_fa42), // -80
    (0x565c_976c_9cbd_fccb, 0x1270_b0df_c1e5_9502), // -79
    (0x4516_df8a_16fe_63d5, 0x5b8d_5a4c_9b1e_10ce), // -78
    (0x6e8a_ff43_57fd_6c89, 0x127b_c3ad_c4fc_e7b0), // -77
    (0x586f_329c_4664_56d4, 0x0ec9_6957_d0ca_52f3), // -76
    (0x46bf_5bb0_3850_4576, 0x3f07_8779_73d5_0f29), // -75
    (0x7132_2c4d_26e6_d58a, 0x31a5_a58f_1fbb_4b75), // -74
    (0x5a8e_89d7_5252_446e, 0x5aea_ead8_e62f_6f91), // -73
    (0x4872_07df_750e_9d25, 0x2f22_557a_51bf_8c74), // -72
    (0x73e9_a632_54e4_2ea2, 0x1836_ef2a_1c65_ad86), // -71
    (0x5cba_eb5b_771c_f21b, 0x2cf8_bf54_e384_8ad2), // -70
    (0x4a2f_22af_927d_8e7c, 0x23fa_32aa_4f9d_3bdb), // -69
    (0x76b1_d118_ea62_7d93, 0x5329_eaaa_18fb_92f8), // -68
    (0x5ef4_a747_21e8_6476, 0x0f54_bbbb_472f_a8c6), // -67
    (0x4bf6_ec38_e7ed_1d2b, 0x25dd_62fc_38f2_ed6c), // -66
    (0x798b_138e_3fe1_c845, 0x22fb_d193_8e51_7bdf), // -65
    (0x613c_0fa4_ffe7_d36a, 0x4f2f_dadc_71da_c97f), // -64
    (0x4dc9_a61d_9986_42bb, 0x58f3_157d_27e2_3acc), // -63
    (0x7c75_d695_c270_6ac5, 0x74b8_2261_d969_f7ad), // -62
    (0x6391_7877_cec0_556b, 0x1093_4eb4_adee_5fbe), // -61
    (0x4fa7_9393_0bcd_1122, 0x4075_d890_8b25_1965), // -60
    (0x7f72_85b8_12e1_b504, 0x00bc_8db4_11d4_f56e), // -59
    (0x65f5_37c6_7581_5d9c, 0x66fd_3e29_a7dd_9125), // -58
    (0x5190_f96b_9134_4ae3, 0x6bfd_cb54_864a_da84), // -57
    (0x4140_c789_40f6_a24f, 0x6ffe_3c43_9ea2_486a), // -56
    (0x6867_a5a8_67f1_03b2, 0x7ffd_2d38_fdd0_73dc), // -55
    (0x5386_1e20_5327_3628, 0x6664_242d_97d9_f64a), // -54
    (0x42d1_b1b3_75b8_f820, 0x51e9_b68a_dfe1_91d5), // -53
    (0x6ae9_1c52_55f4_c034, 0x1ca9_2411_6635_b621), // -52
    (0x5587_49db_77f7_0029, 0x63ba_8341_1e91_5e81), // -51
    (0x446c_3b15_f992_6687, 0x6962_029a_7eda_b201), // -50
    (0x6d79_f823_28ea_3da6, 0x0f03_375d_97c4_5001), // -49
    (0x5794_c682_8721_caeb, 0x259c_2c4a_dfd0_4001), // -48
    (0x4610_9ece_d281_6f22, 0x5149_bd08_b30d_0001), // -47
    (0x701a_97b1_50cf_1837, 0x3542_c80d_eb48_0001), // -46
    (0x59ae_dfc1_0d72_79c5, 0x7768_a00b_22a0_0001), // -45
    (0x47bf_1967_3df5_2e37, 0x7920_8008_e880_0001), // -44
    (0x72cb_5bd8_6321_e38c, 0x5b67_3341_7400_0001), // -43
    (0x5bd5_e313_8281_82d6, 0x7c52_8f67_9000_0001), // -42
    (0x4977_e8dc_6867_9bdf, 0x16a8_72b9_4000_0001), // -41
    (0x758c_a7c7_0d72_92fe, 0x5773_eac2_0000_0001), // -40
    (0x5e0a_1fd2_7128_7598, 0x45f6_5568_0000_0001), // -39
    (0x4b3b_4ca8_5a86_c47a, 0x04c5_1120_0000_0001), // -38
    (0x785e_e10d_5da4_6d90, 0x07a1_b500_0000_0001), // -37
    (0x604b_e73d_e483_8ad9, 0x52e7_c400_0000_0001), // -36
    (0x4d09_85cb_1d36_08ae, 0x0f1f_d000_0000_0001), // -35
    (0x7b42_6fab_61f0_0de3, 0x31cc_8000_0000_0001), // -34
    (0x629b_8c89_1b26_7182, 0x5b0a_0000_0000_0001), // -33
    (0x4ee2_d6d4_15b8_5ace, 0x7c08_0000_0000_0001), // -32
    (0x7e37_be20_22c0_914b, 0x1340_0000_0000_0001), // -31
    (0x64f9_64e6_8233_a76f, 0x2900_0000_0000_0001), // -30
    (0x50c7_83eb_9b5c_85f2, 0x5400_0000_0000_0001), // -29
    (0x409f_9cbc_7c4a_04c2, 0x1000_0000_0000_0001), // -28
    (0x6765_c793_fa10_079d, 0x0000_0000_0000_0001), // -27
    (0x52b7_d2dc_c80c_d2e4, 0x0000_0000_0000_0001), // -26
    (0x422c_a8b0_a00a_4250, 0x0000_0000_0000_0001), // -25
    (0x69e1_0de7_6676_d080, 0x0000_0000_0000_0001), // -24
    (0x54b4_0b1f_852b_da00, 0x0000_0000_0000_0001), // -23
    (0x43c3_3c19_3756_4800, 0x0000_0000_0000_0001), // -22
    (0x6c6b_935b_8bbd_4000, 0x0000_0000_0000_0001), // -21
    (0x56bc_75e2_d631_0000, 0x0000_0000_0000_0001), // -20
    (0x4563_9182_44f4_0000, 0x0000_0000_0000_0001), // -19
    (0x6f05_b59d_3b20_0000, 0x0000_0000_0000_0001), // -18
    (0x58d1_5e17_6280_0000, 0x0000_0000_0000_0001), // -17
    (0x470d_e4df_8200_0000, 0x0000_0000_0000_0001), // -16
    (0x71af_d498_d000_0000, 0x0000_0000_0000_0001), // -15
    (0x5af3_107a_4000_0000, 0x0000_0000_0000_0001), // -14
    (0x48c2_7395_0000_0000, 0x0000_0000_0000_0001), // -13
    (0x746a_5288_0000_0000, 0x0000_0000_0000_0001), // -12
    (0x5d21_dba0_0000_0000, 0x0000_0000_0000_0001), // -11
    (0x4a81_7c80_0000_0000, 0x0000_0000_0000_0001), // -10
    (0x7735_9400_0000_0000, 0x0000_0000_0000_0001), // -9
    (0x5f5e_1000_0000_0000, 0x0000_0000_0000_0001), // -8
    (0x4c4b_4000_0000_0000, 0x0000_0000_0000_0001), // -7
    (0x7a12_0000_0000_0000, 0x0000_0000_0000_0001), // -6
    (0x61a8_0000_0000_0000, 0x0000_0000_0000_0001), // -5
    (0x4e20_0000_0000_0000, 0x0000_0000_0000_0001), // -4
    (0x7d00_0000_0000_0000, 0x0000_0000_0000_0001), // -3
    (0x6400_0000_0000_0000, 0x0000_0000_0000_0001), // -2
    (0x5000_0000_0000_0000, 0x0000_0000_0000_0001), // -1
    (0x4000_0000_0000_0000, 0x0000_0000_0000_0001), // 0
    (0x6666_6666_6666_6666, 0x3333_3333_3333_3334), // 1
    (0x51eb_851e_b851_eb85, 0x0f5c_28f5_c28f_5c29), // 2
    (0x4189_374b_c6a7_ef9d, 0x5916_872b_020c_49bb), // 3
    (0x68db_8bac_710c_b295, 0x74f0_d844_d013_a92b), // 4
    (0x53e2_d623_8da3_c211, 0x43f3_e037_0cdc_8755), // 5
    (0x431b_de82_d7b6_34da, 0x698f_e692_70b0_6c44), // 6
    (0x6b5f_ca6a_f2bd_215e, 0x0f4c_a41d_811a_46d4), // 7
    (0x55e6_3b88_c230_e77e, 0x3f70_834a_cdae_9f10), // 8
    (0x44b8_2fa0_9b5a_52cb, 0x4c5a_02a2_3e25_4c0d), // 9
    (0x6df3_7f67_5ef6_eadf, 0x2d5c_d103_96a2_1347), // 10
    (0x57f5_ff85_e592_557f, 0x3de3_da69_454e_75d3), // 11
    (0x465e_6604_b7a8_4465, 0x7e4f_e1ed_d10b_9175), // 12
    (0x7097_09a1_25da_0709, 0x4a19_697c_81ac_1bef), // 13
    (0x5a12_6e1a_84ae_6c07, 0x54e1_2130_67bc_e326), // 14
    (0x480e_be7b_9d58_566c, 0x43e7_4dc0_52fd_8285), // 15
    (0x734a_ca5f_6226_f0ad, 0x530b_af9a_1e62_6a6d), // 16
    (0x5c3b_d519_1b52_5a24, 0x426f_bfae_7eb5_21f1), // 17
    (0x49c9_7747_490e_ae83, 0x4ebf_cc8b_9890_e7f4), // 18
    (0x760f_253e_db4a_b0d2, 0x4acc_7a78_f41b_0cba), // 19
    (0x5e72_8432_4908_8d75, 0x223d_2ec7_29af_3d62), // 20
    (0x4b8e_d028_3a6d_3df7, 0x34fd_bf05_baf2_9781), // 21
    (0x78e4_8040_5d7b_9658, 0x54c9_31a2_c4b7_58cf), // 22
    (0x60b6_cd00_4ac9_4513, 0x5d6d_c14f_03c5_e0a5), // 23
    (0x4d5f_0a66_a23a_9da9, 0x3124_9aa5_9c9e_4d51), // 24
    (0x7bcb_43d7_69f7_62a8, 0x4ea0_f76f_60fd_4882), // 25
    (0x6309_0312_bb2c_4eed, 0x254d_92bf_80ca_a068), // 26
    (0x4f3a_68db_c8f0_3f24, 0x1dd7_a899_33d5_4d20), // 27
    (0x7ec3_daf9_4180_6506, 0x62f2_a75b_8622_1500), // 28
    (0x6569_7bfa_9acd_1d9f, 0x025b_b916_04e8_10cd), // 29
    (0x5121_2ffb_af0a_7e18, 0x6849_60de_6a53_40a4), // 30
    (0x40e7_5996_25a1_fe7a, 0x203a_b3e5_21dc_33b6), // 31
    (0x67d8_8f56_a29c_ca5d, 0x19f7_863b_6960_52bd), // 32
    (0x5313_a5de_e87d_6eb0, 0x7b2c_6b62_bab3_7564), // 33
    (0x4276_1e4b_ed31_255a, 0x2f56_bc4e_fbc2_c450), // 34
    (0x6a56_96df_e1e8_3bc3, 0x6557_93b1_92d1_3a1a), // 35
    (0x5512_124c_b4b9_c969, 0x3779_42f4_7574_2e7b), // 36
    (0x440e_750a_2a2e_3aba, 0x5f94_3590_5df6_8b96), // 37
    (0x6ce3_ee76_a9e3_912a, 0x65b9_ef4d_6324_1289), // 38
    (0x571c_bec5_54b6_0dbb, 0x6afb_25d7_8283_4207), // 39
    (0x45b0_989d_dd5e_7163, 0x08c8_eb12_cecf_6806), // 40
    (0x6f80_f42f_c897_1bd1, 0x5adb_11b7_b14b_d9a3), // 41
    (0x5933_f68c_a078_e30e, 0x157c_0e2c_8dd6_47b5), // 42
    (0x475c_c53d_4d2d_8271, 0x5dfc_d823_a4ab_6c91), // 43
    (0x722e_0862_1515_9d82, 0x632e_269f_6ddf_141b), // 44
    (0x5b58_06b4_ddaa_e468, 0x4f58_1ee5_f17f_4349), // 45
    (0x4913_3890_b155_8386, 0x72ac_e584_c132_9c3b), // 46
    (0x74eb_8db4_4eef_38d7, 0x6aae_3c07_9b84_2d2a), // 47
    (0x5d89_3e29_d8bf_60ac, 0x5558_3006_1603_5755), // 48
    (0x4ad4_31bb_13cc_4d56, 0x7779_c004_de69_12ab), // 49
    (0x77b9_e92b_52e0_7bbe, 0x258f_99a1_63db_5111), // 50
    (0x5fc7_edbc_424d_2fcb, 0x37a6_1481_1caf_740d), // 51
    (0x4c9f_f163_683d_bfd5, 0x7951_aa00_e3bf_900b), // 52
    (0x7a99_8238_a6c9_32ef, 0x754f_7667_d2cc_19ab), // 53
    (0x6214_682d_523a_8f26, 0x2aa5_f853_0f09_ae22), // 54
    (0x4e76_b9bd_db62_0c1e, 0x5551_9375_a5a1_581b), // 55
    (0x7d8a_c2c9_5f03_4697, 0x3bb5_b8bc_3c35_59c5), // 56
    (0x646f_023a_b269_0545, 0x7c91_6096_9691_149e), // 57
    (0x5058_ce95_5b87_376b, 0x16da_b3ab_aba7_43b2), // 58
    (0x4047_0baa_af9f_5f88, 0x78ae_f622_efb9_02f5), // 59
    (0x66d8_12aa_b298_98db, 0x0de4_bd04_b2c1_9e54), // 60
    (0x5246_7555_5bad_4715, 0x57ea_30d0_8f01_4b76), // 61
    (0x41d1_f777_7c8a_9f44, 0x4654_f3da_0c01_092c), // 62
    (0x694f_f258_c744_3207, 0x23bb_1fc3_4668_0eac), // 63
    (0x543f_f513_d29c_f4d2, 0x4fc8_e635_d1ec_d88a), // 64
    (0x4366_5da9_754a_5d75, 0x263a_51c4_a7f0_ad3b), // 65
    (0x6bd6_fc42_5543_c8bb, 0x56c3_b607_731a_aec4), // 66
    (0x5645_969b_7769_6d62, 0x789c_919f_8f48_8bd0), // 67
    (0x4504_787c_5f87_8ab5, 0x46e3_a7b2_d906_d640), // 68
    (0x6e6d_8d93_cc0c_1122, 0x3e39_0c51_5b3e_239a), // 69
    (0x5857_a476_3cd6_741b, 0x4b60_d6a7_7c31_b615), // 70
    (0x46ac_8391_ca45_29af, 0x55e7_121f_968e_2b44), // 71
    (0x7114_05b6_106e_a919, 0x0971_b698_f0e3_786d), // 72
    (0x5a76_6af8_0d25_5414, 0x078e_2bad_8d82_c6bd), // 73
    (0x485e_bbf9_a41d_dcdc, 0x6c71_bc8a_d79b_d231), // 74
    (0x73ca_c65c_39c9_6161, 0x2d82_c744_8c2c_8382), // 75
    (0x5ca2_3849_c7d4_4de7, 0x3e02_3903_a356_cf9b), // 76
    (0x4a1b_603b_0643_7185, 0x7e68_2d9c_82ab_d949), // 77
    (0x7692_3391_a39f_1c09, 0x4a40_48fa_6aac_8edb), // 78
    (0x5edb_5c74_82e5_b007, 0x5500_3a61_eef0_7249), // 79
    (0x4be2_b05d_3584_8cd2, 0x7733_61e7_f259_f507), // 80
    (0x796a_b3c8_55a0_e151, 0x3eb8_9ca6_508f_ee71), // 81
    (0x6122_296d_114d_810d, 0x7efa_16eb_73a6_585b), // 82
    (0x4db4_edf0_daa4_673e, 0x3261_abef_8fb8_46af), // 83
    (0x7c54_afe7_c43a_3eca, 0x1d69_1318_e5f3_a44b), // 84
    (0x6376_f31f_d02e_98a1, 0x6454_0f47_1e5c_836f), // 85
    (0x4f92_5c19_7358_7a1b, 0x0376_729f_4b7d_35f3), // 86
    (0x7f50_935b_ebc0_c35e, 0x38bd_8432_1261_efeb), // 87
    (0x65da_0f7c_bc9a_35e5, 0x13ca_d028_0eb4_bfef), // 88
    (0x517b_3f96_fd48_2b1d, 0x5ca2_4020_0bc3_ccbf), // 89
    (0x412f_6612_6439_bc17, 0x63b5_0019_a303_0a33), // 90
    (0x684b_d683_d38f_9359, 0x1f88_0029_04d1_a9ea), // 91
    (0x536f_decf_dc72_dc47, 0x32d3_3354_03da_ee55), // 92
    (0x42bf_e573_16c2_49d2, 0x5bdc_2910_0315_8b77), // 93
    (0x6acc_a251_be03_a951, 0x12f9_db4c_d1bc_1258), // 94
    (0x5570_81da_fe69_5440, 0x7594_af70_a7c9_a847), // 95
    (0x445a_017b_feba_a9cd, 0x4476_f2c0_863a_ed06), // 96
    (0x6d5c_cf2c_cac4_42e2, 0x3a57_eacd_a391_7b3c), // 97
    (0x577d_728a_3bd0_3581, 0x7b79_88a4_82da_c8fd), // 98
    (0x45fd_f53b_630c_f79b, 0x15fa_d3b6_cf15_6d97), // 99
    (0x6ffc_bb92_3814_bf5e, 0x565e_1f8a_e4ef_15be), // 100
    (0x5996_fc74_f9aa_32b2, 0x11e4_e608_b725_aaff), // 101
    (0x47ab_fd2a_6154_f55b, 0x27ea_51a0_9284_88cc), // 102
    (0x72ac_c843_ceee_555e, 0x7310_829a_8407_4146), // 103
    (0x5bbd_6d03_0bf1_dde5, 0x4273_9bae_d005_cdd2), // 104
    (0x4964_5735_a327_e4b7, 0x4ec2_e2f2_4004_a4a8), // 105
    (0x756d_5855_d1d9_6df2, 0x4ad1_6b1d_333a_a10c), // 106
    (0x5df1_1377_db14_57f5, 0x2241_227d_c295_4da3), // 107
    (0x4b27_42c6_48dd_132a, 0x4e9a_81fe_3544_3e1c), // 108
    (0x783e_d13d_4161_b844, 0x175d_9cc9_eed3_9694), // 109
    (0x6032_40fd_cde7_c69c, 0x7917_b0a1_8bdc_7876), // 110
    (0x4cf5_00cb_0b1f_d217, 0x1412_f3b4_6fe3_9392), // 111
    (0x7b21_9ade_7832_e9be, 0x5351_85ed_7fd2_85b6), // 112
    (0x6281_48b1_f9c2_5498, 0x42a7_9e57_9975_37c5), // 113
    (0x4ecd_d3c1_949b_76e0, 0x3552_e512_e12a_9304), // 114
    (0x7e16_1f9c_20f8_be33, 0x6eeb_081e_3510_eb39), // 115
    (0x64de_7fb0_1a60_9829, 0x3f22_6ce4_f740_bc2e), // 116
    (0x50b1_ffc0_151a_1354, 0x3281_f0b7_2c33_c9be), // 117
    (0x408e_6633_4414_dc43, 0x4201_8d5f_568f_d498), // 118
    (0x674a_3d1e_d354_939f, 0x1ccf_4898_8a7f_ba8d), // 119
    (0x52a1_ca7f_0f76_dc7f, 0x30a5_d3ad_3b99_620b), // 120
    (0x421b_0865_a5f8_b065, 0x73b7_dc8a_9614_4e6f), // 121
    (0x69c4_da3c_3cc1_1a3c, 0x52bf_c744_2353_b0b1), // 122
    (0x549d_7b63_63cd_ae96, 0x7566_3903_4f76_26f4), // 123
    (0x43b1_2f82_b63e_2545, 0x4451_c735_d92b_525d), // 124
    (0x6c4e_b26a_bd30_3ba2, 0x3a1c_71ef_c1de_ea2e), // 125
    (0x56a5_5b88_9759_c94e, 0x61b0_5b26_34b2_54f2), // 126
    (0x4551_1606_df7b_0772, 0x1af3_7c1e_908e_aa5b), // 127
    (0x6ee8_233e_325e_7250, 0x2b1f_2cfd_b417_76f8), // 128
    (0x58b9_b5cb_5b7e_c1d9, 0x6f4c_23fe_29ac_5f2d), // 129
    (0x46fa_f7d5_e2cb_ce47, 0x72a3_4ffe_87bd_18f1), // 130
    (0x7191_8c89_6adf_b073, 0x0438_7ffd_a5fb_5b1b), // 131
    (0x5ada_d6d4_557f_c05c, 0x0360_6664_84c9_15af), // 132
    (0x48af_1243_7799_66b0, 0x02b3_851d_3707_448c), // 133
    (0x744b_506b_f28f_0ab3, 0x1dec_082e_be72_0746), // 134
    (0x5d09_0d23_2872_6ef5, 0x64bc_d358_985b_3905), // 135
    (0x4a6d_a41c_205b_8bf7, 0x6a30_a913_ad15_c738), // 136
    (0x7715_d360_33c5_acbf, 0x5d1a_a81f_7b56_0b8c), // 137
    (0x5f44_a919_c304_8a32, 0x7dae_ece5_fc44_d609), // 138
    (0x4c36_edae_359d_3b5b, 0x7e25_8a51_969d_7808), // 139
    (0x79f1_7c49_ef61_f893, 0x16a2_76e8_f0fb_f33f), // 140
    (0x618d_fd07_f2b4_c6dc, 0x121b_9253_f3fc_c299), // 141
    (0x4e0b_30d3_2890_9f16, 0x41af_a843_2997_0214), // 142
    (0x7cde_b485_0db4_31bd, 0x4f7f_739e_a8f1_9ced), // 143
    (0x63e5_5d37_3e29_c164, 0x3f99_294b_ba5a_e3f1), // 144
    (0x4fea_b0f8_fe87_cde9, 0x7fad_baa2_fb7b_e98d), // 145
    (0x7fdd_e7f4_ca72_e30f, 0x7f7c_5dd1_925f_dc15), // 146
    (0x664b_1ff7_085b_e8d9, 0x4c63_7e41_41e6_49ab), // 147
    (0x51d5_b32c_06af_ed7a, 0x704f_9834_34b8_3aef), // 148
    (0x4177_c289_9ef3_2462, 0x26a6_135c_f6f9_c8bf), // 149
    (0x68bf_9da8_fe51_d3d0, 0x3dd6_8561_8b29_4132), // 150
    (0x53cc_7e20_cb74_a973, 0x4b12_044e_08ed_cdc2), // 151
    (0x4309_fe80_a2c3_bac2, 0x6f41_9d0b_3a57_d7ce), // 152
    (0x6b43_30cd_d139_2ad1, 0x3202_94de_c3bf_bfb0), // 153
    (0x55cf_5a3e_40fa_88a7, 0x419b_aa4b_cfcc_995a), // 154
    (0x44a5_e1cb_672e_d3b9, 0x1ae2_eea3_0ca3_ade1), // 155
    (0x6dd6_3612_3eb1_52c1, 0x77d1_7dd1_add2_afcf), // 156
    (0x57de_91a8_3227_7567, 0x7974_64a7_be42_263f), // 157
    (0x464b_a7b9_c1b9_2ab9, 0x4790_5086_31ce_84ff), // 158
    (0x7079_0c5c_6928_445c, 0x0c1a_1a70_4fb0_d4cc), // 159
    (0x59fa_7049_edb9_d049, 0x567b_4859_d95a_43d6), // 160
    (0x47fb_8d07_f161_736e, 0x11fc_39e1_7aae_9cab), // 161
    (0x732c_14d9_8235_857d, 0x032d_2968_c44a_9445), // 162
    (0x5c23_43e1_34f7_9dfd, 0x4f57_5453_d03b_a9d1), // 163
    (0x49b5_cfe7_5d92_e4ca, 0x72ac_4376_402f_bb0e), // 164
    (0x75ef_b30b_c8eb_07ab, 0x0446_d256_cd19_2b49), // 165
    (0x5e59_5c09_6d88_d2ef, 0x1d05_7512_3dad_bc3a), // 166
    (0x4b7a_b007_8ad3_dbf2, 0x4a6a_c40e_97be_302f), // 167
    (0x78c4_4cd8_de1f_c650, 0x7711_39b0_f2c9_e6b1), // 168
    (0x609d_0a47_1819_6b73, 0x78da_948d_8f07_ebc1), // 169
    (0x4d4a_6e9f_467a_bc5c, 0x60ae_dd3e_0c06_5634), // 170
    (0x7baa_4a98_70c4_6094, 0x344a_fb96_79a3_bd20), // 171
    (0x62ee_a213_8d69_e6dd, 0x103b_fc78_614f_ca80), // 172
    (0x4f25_4e76_0abb_1f17, 0x2696_6393_810c_a200), // 173
    (0x7ea2_1723_445e_9825, 0x2423_d285_9b47_6999), // 174
    (0x654e_78e9_037e_e01d, 0x69b6_4204_7c39_2148), // 175
    (0x510b_93ed_9c65_8017, 0x6e2b_6803_9694_1aa0), // 176
    (0x40d6_0ff1_49ea_ccdf, 0x71bc_5336_1210_154d), // 177
    (0x67bc_e64e_dcaa_e166, 0x1c60_8523_5019_bbae), // 178
    (0x52fd_850b_e3bb_e784, 0x7d1a_041c_4014_9625), // 179
    (0x4264_6a6f_e963_1f9d, 0x4a7b_367d_0010_781d), // 180
    (0x6a3a_43e6_4238_3295, 0x5d91_f0c8_001a_59c8), // 181
    (0x54fb_6985_01c6_8ede, 0x17a7_f3d3_3348_47d4), // 182
    (0x43fc_546a_67d2_0be4, 0x7953_2975_c2a0_3976), // 183
    (0x6cc6_ed77_0c83_463b, 0x0eeb_7589_3766_c256), // 184
    (0x5705_8ac5_a39c_382f, 0x2589_2ad4_2c52_3512), // 185
    (0x459e_089e_1c7c_f9bf, 0x37a0_ef10_2374_f742), // 186
    (0x6f63_40fc_fa61_8f98, 0x5901_7e80_38bb_2536), // 187
    (0x591c_33fd_951a_d946, 0x7a67_9866_93c8_ea91), // 188
    (0x4749_c331_4415_7a9f, 0x151f_ad1e_dca0_bba8), // 189
    (0x720f_9eb5_39bb_f765, 0x0832_ae97_c767_92a5), // 190
    (0x5b3f_b22a_9496_5f84, 0x068e_f213_05ec_7551), // 191
    (0x48ff_c1bb_aa11_e603, 0x1ed8_c1a8_d189_f774), // 192
    (0x74cc_692c_434f_d66b, 0x4af4_690e_1c0f_f253), // 193
    (0x5d70_5423_690c_ab89, 0x225d_20d8_1673_2843), // 194
    (0x4ac0_434f_873d_5607, 0x3517_4d79_ab8f_5369), // 195
    (0x779a_054c_0b95_5672, 0x21be_e25c_45b2_1f0e), // 196
    (0x5fae_6aa3_3c77_785b, 0x3498_b516_9e28_18d8), // 197
    (0x4c8b_8882_96c5_f9e2, 0x5d46_f745_4b53_4713), // 198
    (0x7a78_da6a_8ad6_5c9d, 0x7ba4_bed5_4552_0b52), // 199
    (0x61fa_4855_3bde_b07e, 0x2fb6_ff11_0441_a2a8), // 200
    (0x4e61_d377_6318_8d31, 0x72f8_cc0d_9d01_4eed), // 201
    (0x7d69_5258_9e8d_aeb6, 0x1e5a_e015_c802_17e1), // 202
    (0x6454_41e0_7ed7_bef8, 0x1848_b344_a001_acb4), // 203
    (0x5043_67e6_cbdf_cbf9, 0x603a_2903_b334_8a2a), // 204
    (0x4035_ecb8_a319_6ffb, 0x002e_8736_28f6_d4ee), // 205
    (0x66bc_adf4_3828_b32b, 0x19e4_0b89_db24_87e3), // 206
    (0x5230_8b29_c686_f5bc, 0x14b6_6fa1_7c1d_3983), // 207
    (0x41c0_6f54_9ed2_5e30, 0x1091_f2e7_967d_c79c), // 208
    (0x6933_e554_3150_96b3, 0x341c_b7d8_f0c9_3f5f), // 209
    (0x5429_8443_5aa6_def5, 0x767d_5fe0_c0a0_ff80), // 210
    (0x4354_69cf_7bb8_b25e, 0x2b97_7fe7_0080_cc66), // 211
    (0x6bba_42e5_92c1_1d63, 0x5f58_cca4_cd9a_e0a3), // 212
    (0x562e_9bea_dbcd_b11c, 0x4c47_0a1d_7148_b3b6), // 213
    (0x44f2_1655_7ca4_8db0, 0x3d05_a1b1_276d_5c92), // 214
    (0x6e50_23bb_faa0_e2b3, 0x7b3c_35e8_3f15_60e9), // 215
    (0x5840_1c96_621a_4ef6, 0x2f63_5e53_65aa_b3ed), // 216
    (0x4699_b078_4e7b_725e, 0x591c_4b75_eaee_f658), // 217
    (0x70f5_e726_e3f8_b6fd, 0x74fa_1256_44b1_8a26), // 218
    (0x5a5e_5285_832d_5f31, 0x43fb_41de_9d5a_d4eb), // 219
    (0x484b_7537_9c24_4c27, 0x4ffc_34b2_177b_dd89), // 220
    (0x73ab_eebf_603a_1372, 0x4cc6_bab6_8bf9_6274), // 221
    (0x5c89_8bcc_4cfb_42c2, 0x0a38_955e_d661_1b90), // 222
    (0x4a07_a309_d72f_689b, 0x21c6_dde5_784d_afa7), // 223
    (0x7672_9e76_2518_a75e, 0x693e_2fd5_8d49_190b), // 224
    (0x5ec2_185e_8413_b918, 0x5431_bfde_0aa0_e0d5), // 225
    (0x4bce_79e5_3676_2dad, 0x29c1_664b_3bb3_e711), // 226
    (0x794a_5ca1_f0bd_15e2, 0x0f9b_d6de_c5ec_a4e8), // 227
    (0x6108_4a1b_26fd_ab1b, 0x2616_457f_04bd_50ba), // 228
    (0x4da0_3b48_ebfe_227c, 0x1e78_3798_d097_73c8), // 229
    (0x7c33_920e_4663_6a60, 0x30c0_58f4_80f2_52d9), // 230
    (0x635c_74d8_384f_884d, 0x0d66_ad90_6728_4247), // 231
    (0x4f7d_2a46_9372_d370, 0x711e_f140_5286_9b6c), // 232
    (0x7f2e_aa0a_8584_8581, 0x34fe_4ecd_50d7_5f14), // 233
    (0x65be_ee6e_d136_d134, 0x2a65_0bd7_73df_7f43), // 234
    (0x5165_8b8b_da92_40f6, 0x551d_a312_c319_329c), // 235
    (0x411e_093c_aedb_672b, 0x5db1_4f42_35ad_c217), // 236
    (0x6830_0ec7_7e2b_d845, 0x7c4e_e536_bc49_368a), // 237
    (0x5359_a56c_64ef_e037, 0x7d0b_ea92_303a_9208), // 238
    (0x42ae_1df0_50bf_e693, 0x173c_bba8_2695_41a0), // 239
    (0x6ab0_2fe6_e799_70eb, 0x3ec7_92a6_a422_029a), // 240
    (0x5559_bfeb_ec7a_c0bc, 0x3239_421e_e9b4_cee1), // 241
    (0x4447_ccbc_bd2f_0096, 0x5b61_01b2_5490_a581), // 242
    (0x6d3f_adfa_c84b_3424, 0x2bce_691d_541a_a268), // 243
    (0x5766_24c8_a03c_29b6, 0x563e_ba7d_dce2_1b87), // 244
    (0x45eb_50a0_8030_215e, 0x7832_2ecb_171b_4939), // 245
    (0x6fde_e767_3380_3564, 0x59e9_e478_24f8_7527), // 246
    (0x597f_1f85_c2cc_f783, 0x6187_e9f9_b72d_2a86), // 247
    (0x4798_e604_9bd7_2c69, 0x346c_bb2e_2c24_2205), // 248
    (0x728e_3cd4_2c8b_7a42, 0x20ad_f849_e039_d007), // 249
    (0x5ba4_fd76_8a09_2e9b, 0x33be_603b_19c7_d99f), // 250
    (0x4950_cac5_3b3a_8baf, 0x42fe_b362_7b06_47b3), // 251
    (0x754e_113b_91f7_45e5, 0x5197_856a_5e70_72b8), // 252
    (0x5dd8_0dc9_4192_9e51, 0x27ac_6abb_7ec0_5bc6), // 253
    (0x4b13_3e3a_9adb_b1da, 0x52f0_5562_cbcd_1638), // 254
    (0x781e_c9f7_5e2c_4fc4, 0x1e4d_556a_dfae_89f3), // 255
    (0x6018_a192_b1bd_0c9c, 0x7ea4_4455_7fbe_d4c3), // 256
    (0x4ce0_8142_27ca_707d, 0x4bb6_9d11_32ff_109c), // 257
    (0x7b00_ced0_3faa_4d95, 0x5f8a_94e8_5198_1a93), // 258
    (0x6267_0bd9_cc88_3e11, 0x32d5_43ed_0e13_4875), // 259
    (0x4eb8_d647_d6d3_64da, 0x5bdd_cff0_d80f_6d2b), // 260
    (0x7df4_8a0c_8aeb_d491, 0x12fc_7fe7_c018_aeab), // 261
    (0x64c3_a1a3_a256_43a7, 0x28c9_ffec_99ad_5889), // 262
    (0x509c_814f_b511_cfb9, 0x0707_fff0_7af1_13a1), // 263
    (0x407d_343f_c40e_3fc7, 0x1f39_998d_2f27_42e7), // 264
    (0x672e_b9ff_a016_cc71, 0x7ec2_8f48_4b72_04a4), // 265
    (0x528b_c7ff_b345_705b, 0x189b_a5d3_6f8e_6a1d), // 266
    (0x4209_6ccc_8f6a_c048, 0x7a16_1e42_bfa5_21b1), // 267
    (0x69a8_ae14_18aa_cd41, 0x4356_96d1_32a1_cf81), // 268
    (0x5486_f1a9_ad55_7101, 0x1c45_4574_2881_72ce), // 269
    (0x439f_27ba_f111_2734, 0x169d_d129_ba01_28a5), // 270
    (0x6c31_d92b_1b4e_a520, 0x242f_b50f_9001_daa1), // 271
    (0x568e_4755_af72_1db3, 0x368c_90d9_4001_7bb4), // 272
    (0x453e_9f77_bf8e_7e29, 0x120a_0d7a_999a_c95d), // 273
    (0x6eca_98bf_98e3_fd0e, 0x5010_1590_f5c4_7561), // 274
    (0x58a2_13cc_7a4f_fda5, 0x2673_4473_f7d0_5de8), // 275
    (0x46e8_0fd6_c83f_fe1d, 0x6b8f_69f6_5fd9_e4b9), // 276
    (0x7173_4c8a_d9ff_fcfc, 0x45b2_4323_cc8f_d45c), // 277
    (0x5ac2_a3a2_47ff_fd96, 0x6af5_0283_0a0c_a9e3), // 278
    (0x489b_b61b_6ccc_cadf, 0x08c4_0202_6e70_87e9), // 279
    (0x742c_5692_47ae_1164, 0x746c_d003_e3e7_3fdb), // 280
    (0x5cf0_4541_d2f1_a783, 0x76bd_7336_4fec_3315), // 281
    (0x4a59_d101_758e_1f9c, 0x5efd_f5c5_0cbc_f5ab), // 282
    (0x76f6_1b35_88e3_65c7, 0x4b2f_efa1_adfb_22ab), // 283
    (0x5f2b_48f7_a0b5_eb06, 0x08f3_261a_f195_b555), // 284
    (0x4c22_a0c6_1a2b_226b, 0x20c2_84e2_5ade_2aab), // 285
    (0x79d1_013c_f6ab_6a45, 0x1ad0_d49d_5e30_4444), // 286
    (0x6174_00fd_9222_bb6a, 0x48a7_107d_e4f3_69d0), // 287
    (0x4df6_6731_41b5_62bb, 0x53b8_d9fe_50c2_bb0d), // 288
    (0x7cbd_71e8_6922_3792, 0x52c1_5cca_1ad1_2b48), // 289
    (0x63ca_c186_ba81_c60e, 0x7567_7d6e_7bda_8906), // 290
    (0x4fd5_679e_fb9b_04d8, 0x5dec_6458_6315_3a6c), // 291
    (0x7fbb_d8fe_5f5e_6e27, 0x497a_3a27_04ee_c3df), // 292
];
