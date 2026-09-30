// port of: java.util.regex.Pattern (subset used by JEXL)
//! `java.util.regex.Pattern`, `Matcher` and `PatternSyntaxException`, reproduced on top of
//! `fancy-regex`.
//!
//! The parser is a structural port of the JDK's `Pattern.compile`/`expr`/`sequence`/`atom`/
//! `escape`/`clazz`/`range`/`family`/`group0`/`closure` recursive descent, so its
//! `PatternSyntaxException` descriptions and indices fall out of the same control flow rather
//! than being reverse-engineered from messages.  Instead of building the JDK's `Node` tree it
//! emits an equivalent fancy-regex pattern: every Java-specific semantic (ASCII-only `\d`,
//! `.` excluding line terminators, `$` before a final terminator, ASCII-only
//! case-insensitivity, POSIX classes, Unicode blocks) is baked into the emitted text, so no
//! fancy-regex flag is ever relied on.
//!
//! Known divergences are collected in `UNSUPPORTED_*` below; each one surfaces as a
//! `PatternSyntaxException` whose description starts with `Unsupported`.

use fancy_regex::Regex;
use std::fmt;

pub const UNIX_LINES: i32 = 0x01;
pub const CASE_INSENSITIVE: i32 = 0x02;
pub const COMMENTS: i32 = 0x04;
pub const MULTILINE: i32 = 0x08;
pub const LITERAL: i32 = 0x10;
pub const DOTALL: i32 = 0x20;
pub const UNICODE_CASE: i32 = 0x40;
pub const CANON_EQ: i32 = 0x80;
pub const UNICODE_CHARACTER_CLASS: i32 = 0x100;

const ALL_FLAGS: i32 = CASE_INSENSITIVE
    | MULTILINE
    | DOTALL
    | UNIX_LINES
    | COMMENTS
    | UNICODE_CASE
    | CANON_EQ
    | LITERAL
    | UNICODE_CHARACTER_CLASS;

/// Backtracking budget handed to fancy-regex.  The JDK has no such limit: where a Java
/// `Matcher` would spin (catastrophic backtracking), this port reports "no match" instead.
const BACKTRACK_LIMIT: usize = 1_000_000;

// --------------------------------------------------------------------- exceptions

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PatternSyntaxException {
    desc: String,
    pattern: String,
    index: i32,
}

impl PatternSyntaxException {
    fn new(desc: impl Into<String>, pattern: &str, index: i32) -> Self {
        PatternSyntaxException { desc: desc.into(), pattern: pattern.to_string(), index }
    }

    /// Byte-for-byte `PatternSyntaxException.getMessage()` (line separator `\n`).
    pub fn get_message(&self) -> String {
        let mut s = self.desc.clone();
        if self.index >= 0 {
            s.push_str(" near index ");
            s.push_str(&self.index.to_string());
        }
        s.push('\n');
        s.push_str(&self.pattern);
        // Java compares against String.length(), i.e. UTF-16 code units.
        let u16_len: usize = self.pattern.chars().map(char::len_utf16).sum();
        if self.index >= 0 && (self.index as usize) < u16_len {
            s.push('\n');
            for _ in 0..self.index {
                s.push(' ');
            }
            s.push('^');
        }
        s
    }
    pub fn get_description(&self) -> &str {
        &self.desc
    }
    pub fn get_index(&self) -> i32 {
        self.index
    }
    pub fn get_pattern(&self) -> &str {
        &self.pattern
    }
}

impl fmt::Display for PatternSyntaxException {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.get_message())
    }
}

/// The three exception types the Pattern/Matcher/String surface can raise.  The payload is
/// the Java `getMessage()` verbatim.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JavaRegexError {
    Syntax(PatternSyntaxException),
    IllegalArgument(String),
    IndexOutOfBounds(String),
}

impl fmt::Display for JavaRegexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JavaRegexError::Syntax(e) => f.write_str(&e.get_message()),
            JavaRegexError::IllegalArgument(m) | JavaRegexError::IndexOutOfBounds(m) => {
                f.write_str(m)
            }
        }
    }
}

impl From<PatternSyntaxException> for JavaRegexError {
    fn from(e: PatternSyntaxException) -> Self {
        JavaRegexError::Syntax(e)
    }
}

type PResult<T> = Result<T, PatternSyntaxException>;

// --------------------------------------------------------------------- char classes

/// A Java character-class predicate, kept as a set expression so that `&&`, nesting and
/// negation translate to regex-syntax's own set operators.
#[derive(Clone, Debug)]
enum Cc {
    /// Explicit code point ranges (already case-closed where Java would be).
    Set(Vec<(u32, u32)>),
    /// A ready-made fancy-regex class body, e.g. `\p{L}\p{Nd}`.
    Body(String),
    Union(Box<Cc>, Box<Cc>),
    And(Box<Cc>, Box<Cc>),
    Neg(Box<Cc>),
}

impl Cc {
    fn set(ranges: &[(u32, u32)]) -> Cc {
        Cc::Set(ranges.to_vec())
    }
    fn body(s: &str) -> Cc {
        Cc::Body(s.to_string())
    }
    fn union(self, other: Cc) -> Cc {
        Cc::Union(Box::new(self), Box::new(other))
    }
    fn and(self, other: Cc) -> Cc {
        Cc::And(Box::new(self), Box::new(other))
    }
    fn negate(self) -> Cc {
        Cc::Neg(Box::new(self))
    }

    /// Renders this predicate as the *body* of a `[...]`.  `None` means "matches nothing"
    /// (a class that only held surrogates, which a Rust `str` can never contain).
    fn body_text(&self) -> Option<String> {
        match self {
            Cc::Set(ranges) => {
                let mut out = String::new();
                for &(lo, hi) in ranges {
                    for (lo, hi) in drop_surrogates(lo, hi) {
                        if lo == hi {
                            out.push_str(&hex(lo));
                        } else {
                            out.push_str(&hex(lo));
                            out.push('-');
                            out.push_str(&hex(hi));
                        }
                    }
                }
                if out.is_empty() {
                    None
                } else {
                    Some(out)
                }
            }
            Cc::Body(s) => Some(s.clone()),
            Cc::Union(a, b) => match (a.body_text(), b.body_text()) {
                (None, x) | (x, None) => x,
                (Some(x), Some(y)) => Some(format!("{x}{y}")),
            },
            Cc::And(a, b) => match (a.body_text(), b.body_text()) {
                (Some(x), Some(y)) => Some(format!("[{x}]&&[{y}]")),
                _ => None,
            },
            Cc::Neg(a) => match a.body_text() {
                None => Some(format!("{}-{}", hex(0), hex(0x10_FFFF))),
                Some(x) => Some(format!("[^{x}]")),
            },
        }
    }

    /// Renders this predicate as a standalone node.
    fn node(&self, ci_unicode: bool) -> String {
        match self.body_text() {
            // A class that can never match: a negative look-ahead on the empty string.
            None => "(?!)".to_string(),
            Some(b) if ci_unicode => format!("(?i:[{b}])"),
            Some(b) => format!("[{b}]"),
        }
    }
}

/// Removes the surrogate range; Rust strings cannot contain those code points, so a class
/// made only of surrogates matches nothing.
fn drop_surrogates(lo: u32, hi: u32) -> Vec<(u32, u32)> {
    let mut out = Vec::new();
    if lo < 0xD800 {
        out.push((lo, hi.min(0xD7FF)));
    }
    if hi > 0xDFFF {
        out.push((lo.max(0xE000), hi));
    }
    out
}

fn hex(cp: u32) -> String {
    format!("\\x{{{cp:X}}}")
}

/// A single code point as regex text (safe everywhere, inside classes included).
fn lit(cp: u32) -> String {
    if cp < 128 && (cp as u8 as char).is_ascii_alphanumeric() {
        (cp as u8 as char).to_string()
    } else {
        hex(cp)
    }
}

fn ascii_lower(cp: u32) -> u32 {
    if (b'A' as u32..=b'Z' as u32).contains(&cp) { cp + 32 } else { cp }
}

fn ascii_upper(cp: u32) -> u32 {
    if (b'a' as u32..=b'z' as u32).contains(&cp) { cp - 32 } else { cp }
}

/// Java's `CIRange`: the range, plus its ASCII case-swapped image.
fn ascii_ci_ranges(lo: u32, hi: u32) -> Vec<(u32, u32)> {
    let mut out = vec![(lo, hi)];
    let (a, z) = (b'a' as u32, b'z' as u32);
    let (ua, uz) = (b'A' as u32, b'Z' as u32);
    if lo.max(a) <= hi.min(z) {
        out.push((lo.max(a) - 32, hi.min(z) - 32));
    }
    if lo.max(ua) <= hi.min(uz) {
        out.push((lo.max(ua) + 32, hi.min(uz) + 32));
    }
    out
}

// --------------------------------------------------------------------- property tables

include!("regex_blocks.rs");

fn norm_block(name: &str) -> String {
    name.chars()
        .filter(|c| *c != ' ' && *c != '_' && *c != '-')
        .flat_map(char::to_uppercase)
        .collect()
}

fn for_unicode_block(name: &str) -> Option<Cc> {
    let want = norm_block(name);
    let ranges: Vec<(u32, u32)> =
        BLOCKS.iter().filter(|(n, _, _)| norm_block(n) == want).map(|&(_, l, h)| (l, h)).collect();
    if ranges.is_empty() { None } else { Some(Cc::Set(ranges)) }
}

/// `Character.UnicodeScript.forName` + `\p{Script=...}`.  Validity is decided by
/// regex-syntax, which uses the same UCD aliases.
fn for_unicode_script(name: &str) -> Option<Cc> {
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }
    let body = format!("\\p{{Script={name}}}");
    Regex::new(&format!("[{body}]")).ok().map(|_| Cc::Body(body))
}

// Predicates from CharPredicates.java that the JDK shares between `\p{Is...}` and `\d`/`\w`.
fn p_alphabetic() -> Cc {
    Cc::body("\\p{Alphabetic}")
}
fn p_digit() -> Cc {
    Cc::body("\\p{Nd}")
}
fn p_white_space() -> Cc {
    Cc::body("\\p{Zs}\\p{Zl}\\p{Zp}\\x{9}-\\x{D}\\x{85}")
}
fn p_word() -> Cc {
    p_alphabetic().union(Cc::body("\\p{Mn}\\p{Me}\\p{Mc}\\p{Nd}\\p{Pc}\\x{200C}\\x{200D}"))
}
fn p_control() -> Cc {
    Cc::body("\\p{Cc}")
}
fn p_graph() -> Cc {
    // [^ \p{Z} \p{Cc} \p{Cs} \p{Cn} ]; surrogates are unreachable in a Rust str.
    Cc::body("\\p{Zs}\\p{Zl}\\p{Zp}\\p{Cc}\\p{Cn}").negate()
}
fn p_blank() -> Cc {
    Cc::body("\\p{Zs}\\x{9}")
}
fn p_hex_digit() -> Cc {
    p_digit().union(Cc::body(
        "0-9A-Fa-f\\x{FF10}-\\x{FF19}\\x{FF21}-\\x{FF26}\\x{FF41}-\\x{FF46}",
    ))
}
fn p_noncharacter() -> Cc {
    let mut r = vec![(0xFDD0, 0xFDEF)];
    for plane in 0..=0x10u32 {
        r.push((plane * 0x10000 + 0xFFFE, plane * 0x10000 + 0xFFFF));
    }
    Cc::Set(r)
}

/// `CharPredicates.getPosixPredicate` (the Unicode-flavoured POSIX names).
fn posix_predicate(name: &str, ci: bool) -> Option<Cc> {
    let lower = Cc::body("\\p{Lowercase}");
    let upper = Cc::body("\\p{Uppercase}");
    let title = Cc::body("\\p{Lt}");
    Some(match name {
        "ALPHA" => p_alphabetic(),
        "LOWER" => {
            if ci {
                lower.union(upper).union(title)
            } else {
                lower
            }
        }
        "UPPER" => {
            if ci {
                upper.union(lower).union(title)
            } else {
                upper
            }
        }
        "SPACE" => p_white_space(),
        "PUNCT" => Cc::body("\\p{P}"),
        "XDIGIT" => p_hex_digit(),
        "ALNUM" => p_alphabetic().union(p_digit()),
        "CNTRL" => p_control(),
        "DIGIT" => p_digit(),
        "BLANK" => p_blank(),
        "GRAPH" => p_graph(),
        "PRINT" => p_graph().union(p_blank()).and(p_control().negate()),
        _ => return None,
    })
}

/// `CharPredicates.getUnicodePredicate`.
fn unicode_predicate(name: &str, ci: bool) -> Option<Cc> {
    let lower = Cc::body("\\p{Lowercase}");
    let upper = Cc::body("\\p{Uppercase}");
    let title = Cc::body("\\p{Lt}");
    Some(match name {
        "ALPHABETIC" => p_alphabetic(),
        "ASSIGNED" => Cc::body("\\p{Cn}").negate(),
        "CONTROL" => p_control(),
        "HEXDIGIT" | "HEX_DIGIT" => p_hex_digit(),
        "IDEOGRAPHIC" => Cc::body("\\p{Ideographic}"),
        "JOINCONTROL" | "JOIN_CONTROL" => Cc::set(&[(0x200C, 0x200D)]),
        "LETTER" => Cc::body("\\p{L}"),
        "LOWERCASE" => {
            if ci {
                lower.union(upper).union(title)
            } else {
                lower
            }
        }
        "NONCHARACTERCODEPOINT" | "NONCHARACTER_CODE_POINT" => p_noncharacter(),
        "TITLECASE" => {
            if ci {
                title.union(lower).union(upper)
            } else {
                title
            }
        }
        "PUNCTUATION" => Cc::body("\\p{P}"),
        "UPPERCASE" => {
            if ci {
                upper.union(lower).union(title)
            } else {
                upper
            }
        }
        "WHITESPACE" | "WHITE_SPACE" => p_white_space(),
        "WORD" => p_word(),
        _ => return None,
    })
}

/// `CharPredicates.forProperty` - general categories, POSIX ASCII classes and the
/// `java*` `Character` methods.
fn for_property(name: &str, ci: bool) -> Option<Cc> {
    let cat = |s: &str| Cc::body(s);
    let cased = |s: &str| {
        if ci {
            Cc::body("\\p{Lu}\\p{Ll}\\p{Lt}")
        } else {
            Cc::body(s)
        }
    };
    Some(match name {
        // --- general categories
        "Cn" => cat("\\p{Cn}"),
        "Lu" => cased("\\p{Lu}"),
        "Ll" => cased("\\p{Ll}"),
        "Lt" => cased("\\p{Lt}"),
        "Lm" => cat("\\p{Lm}"),
        "Lo" => cat("\\p{Lo}"),
        "Mn" => cat("\\p{Mn}"),
        "Me" => cat("\\p{Me}"),
        "Mc" => cat("\\p{Mc}"),
        "Nd" => cat("\\p{Nd}"),
        "Nl" => cat("\\p{Nl}"),
        "No" => cat("\\p{No}"),
        "Zs" => cat("\\p{Zs}"),
        "Zl" => cat("\\p{Zl}"),
        "Zp" => cat("\\p{Zp}"),
        "Cc" => cat("\\p{Cc}"),
        "Cf" => cat("\\p{Cf}"),
        "Co" => cat("\\p{Co}"),
        "Cs" => Cc::set(&[(0xD800, 0xDFFF)]),
        "Pd" => cat("\\p{Pd}"),
        "Ps" => cat("\\p{Ps}"),
        "Pe" => cat("\\p{Pe}"),
        "Pc" => cat("\\p{Pc}"),
        "Po" => cat("\\p{Po}"),
        "Sm" => cat("\\p{Sm}"),
        "Sc" => cat("\\p{Sc}"),
        "Sk" => cat("\\p{Sk}"),
        "So" => cat("\\p{So}"),
        "Pi" => cat("\\p{Pi}"),
        "Pf" => cat("\\p{Pf}"),
        "L" => cat("\\p{L}"),
        "M" => cat("\\p{M}"),
        "N" => cat("\\p{N}"),
        "Z" => cat("\\p{Z}"),
        "C" => Cc::body("\\p{Cc}\\p{Cf}\\p{Co}\\p{Cn}").union(Cc::set(&[(0xD800, 0xDFFF)])),
        "P" => cat("\\p{P}"),
        "S" => cat("\\p{S}"),
        "LC" => cat("\\p{Lu}\\p{Ll}\\p{Lt}"),
        "LD" => cat("\\p{L}\\p{Nd}"),
        "L1" => Cc::set(&[(0x00, 0xFF)]),
        "all" => Cc::set(&[(0x00, 0x10_FFFF)]),
        // --- POSIX, ASCII-only
        "ASCII" => Cc::set(&[(0x00, 0x7F)]),
        "Alnum" => Cc::set(&[(0x30, 0x39), (0x41, 0x5A), (0x61, 0x7A)]),
        "Alpha" => Cc::set(&[(0x41, 0x5A), (0x61, 0x7A)]),
        "Blank" => Cc::set(&[(0x09, 0x09), (0x20, 0x20)]),
        "Cntrl" => Cc::set(&[(0x00, 0x1F), (0x7F, 0x7F)]),
        "Digit" => Cc::set(&[(0x30, 0x39)]),
        "Graph" => Cc::set(&[(0x21, 0x7E)]),
        "Lower" => {
            if ci {
                Cc::set(&[(0x41, 0x5A), (0x61, 0x7A)])
            } else {
                Cc::set(&[(0x61, 0x7A)])
            }
        }
        "Print" => Cc::set(&[(0x20, 0x7E)]),
        "Punct" => Cc::set(&[(0x21, 0x2F), (0x3A, 0x40), (0x5B, 0x60), (0x7B, 0x7E)]),
        "Space" => Cc::set(&[(0x09, 0x0D), (0x20, 0x20)]),
        "Upper" => {
            if ci {
                Cc::set(&[(0x41, 0x5A), (0x61, 0x7A)])
            } else {
                Cc::set(&[(0x41, 0x5A)])
            }
        }
        "XDigit" => Cc::set(&[(0x30, 0x39), (0x41, 0x46), (0x61, 0x66)]),
        // --- java.lang.Character predicates
        "javaLowerCase" => {
            if ci {
                Cc::body("\\p{Lowercase}\\p{Uppercase}\\p{Lt}")
            } else {
                Cc::body("\\p{Lowercase}")
            }
        }
        "javaUpperCase" => {
            if ci {
                Cc::body("\\p{Lowercase}\\p{Uppercase}\\p{Lt}")
            } else {
                Cc::body("\\p{Uppercase}")
            }
        }
        "javaTitleCase" => {
            if ci {
                Cc::body("\\p{Lowercase}\\p{Uppercase}\\p{Lt}")
            } else {
                Cc::body("\\p{Lt}")
            }
        }
        "javaAlphabetic" => p_alphabetic(),
        "javaIdeographic" => Cc::body("\\p{Ideographic}"),
        "javaDigit" => p_digit(),
        "javaDefined" => Cc::body("\\p{Cn}").negate(),
        "javaLetter" => Cc::body("\\p{L}"),
        "javaLetterOrDigit" => Cc::body("\\p{L}\\p{Nd}"),
        "javaJavaIdentifierStart" => Cc::body("\\p{L}\\p{Nl}\\p{Sc}\\p{Pc}"),
        "javaJavaIdentifierPart" => Cc::body(
            "\\p{L}\\p{Nl}\\p{Sc}\\p{Pc}\\p{Mn}\\p{Mc}\\p{Nd}\\p{Cf}\
             \\x{0}-\\x{8}\\x{E}-\\x{1B}\\x{7F}-\\x{9F}",
        ),
        "javaUnicodeIdentifierStart" => Cc::body("\\p{ID_Start}"),
        "javaUnicodeIdentifierPart" => Cc::body(
            "\\p{ID_Continue}\\p{Cf}\\x{0}-\\x{8}\\x{E}-\\x{1B}\\x{7F}-\\x{9F}",
        ),
        "javaIdentifierIgnorable" => {
            Cc::body("\\p{Cf}\\x{0}-\\x{8}\\x{E}-\\x{1B}\\x{7F}-\\x{9F}")
        }
        "javaSpaceChar" => Cc::body("\\p{Z}"),
        "javaWhitespace" => Cc::body(
            "[\\p{Z}--[\\x{A0}\\x{2007}\\x{202F}]]\\x{9}-\\x{D}\\x{1C}-\\x{1F}",
        ),
        "javaISOControl" => Cc::set(&[(0x00, 0x1F), (0x7F, 0x9F)]),
        _ => return None,
    })
}

// --------------------------------------------------------------------- parser

/// `escape()`'s three-way return: a code point, a ready node, a class predicate, or the
/// JDK's `-1` with `create == false` (nothing produced).
enum Esc {
    Ch(u32),
    Node(String),
    Class(Cc),
    Nothing,
}

struct Parser<'a> {
    original: &'a str,
    temp: Vec<u32>,
    pattern_length: usize,
    cursor: usize,
    flags0: i32,
    capturing_group_count: usize,
    named_groups: Vec<(String, usize)>,
    depth: u32,
    /// Set by `u()`, whose JDK counterpart throws from a value-returning position.
    pending_error: Option<PatternSyntaxException>,
}

const MAX_DEPTH: u32 = 200;

impl<'a> Parser<'a> {
    fn has(&self, f: i32) -> bool {
        self.flags0 & f != 0
    }

    fn tget(&self, i: usize) -> u32 {
        self.temp.get(i).copied().unwrap_or(0)
    }

    fn error<T>(&self, s: impl Into<String>) -> PResult<T> {
        Err(PatternSyntaxException::new(s, self.original, self.cursor as i32 - 1))
    }

    fn accept(&mut self, ch: u32, s: &str) -> PResult<()> {
        let mut test = self.tget(self.cursor);
        self.cursor += 1;
        if self.has(COMMENTS) {
            test = self.parse_past_whitespace(test);
        }
        if ch != test {
            return self.error(s);
        }
        Ok(())
    }

    fn mark(&mut self, c: u32) {
        if self.pattern_length < self.temp.len() {
            self.temp[self.pattern_length] = c;
        }
    }

    fn peek(&mut self) -> u32 {
        let ch = self.tget(self.cursor);
        if self.has(COMMENTS) { self.peek_past_whitespace(ch) } else { ch }
    }

    fn read(&mut self) -> u32 {
        let ch = self.tget(self.cursor);
        self.cursor += 1;
        if self.has(COMMENTS) { self.parse_past_whitespace(ch) } else { ch }
    }

    fn next(&mut self) -> u32 {
        self.cursor += 1;
        let ch = self.tget(self.cursor);
        if self.has(COMMENTS) { self.peek_past_whitespace(ch) } else { ch }
    }

    fn next_escaped(&mut self) -> u32 {
        self.cursor += 1;
        self.tget(self.cursor)
    }

    fn peek_past_whitespace(&mut self, mut ch: u32) -> u32 {
        while ascii_is_space(ch) || ch == '#' as u32 {
            while ascii_is_space(ch) {
                self.cursor += 1;
                ch = self.tget(self.cursor);
            }
            if ch == '#' as u32 {
                ch = self.peek_past_line();
            }
        }
        ch
    }

    fn parse_past_whitespace(&mut self, mut ch: u32) -> u32 {
        while ascii_is_space(ch) || ch == '#' as u32 {
            while ascii_is_space(ch) {
                ch = self.tget(self.cursor);
                self.cursor += 1;
            }
            if ch == '#' as u32 {
                ch = self.parse_past_line();
            }
        }
        ch
    }

    fn parse_past_line(&mut self) -> u32 {
        let mut ch = self.tget(self.cursor);
        self.cursor += 1;
        while ch != 0 && !self.is_line_separator(ch) {
            ch = self.tget(self.cursor);
            self.cursor += 1;
        }
        if ch == 0 && self.cursor > self.pattern_length {
            self.cursor = self.pattern_length;
            ch = self.tget(self.cursor);
            self.cursor += 1;
        }
        ch
    }

    fn peek_past_line(&mut self) -> u32 {
        self.cursor += 1;
        let mut ch = self.tget(self.cursor);
        while ch != 0 && !self.is_line_separator(ch) {
            self.cursor += 1;
            ch = self.tget(self.cursor);
        }
        if ch == 0 && self.cursor > self.pattern_length {
            self.cursor = self.pattern_length;
            ch = self.tget(self.cursor);
        }
        ch
    }

    fn is_line_separator(&self, ch: u32) -> bool {
        if self.has(UNIX_LINES) {
            ch == 0x0A
        } else {
            ch == 0x0A || ch == 0x0D || (ch | 1) == 0x2029 || ch == 0x85
        }
    }

    fn skip(&mut self) -> u32 {
        let i = self.cursor;
        let ch = self.tget(i + 1);
        self.cursor = i + 2;
        ch
    }

    fn unread(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    // ------------------------------------------------------------- emission helpers

    /// `Pattern.single()` - a literal code point under the current case flags.
    fn emit_single(&self, cp: u32) -> String {
        if char::from_u32(cp).is_none() {
            return "(?!)".to_string(); // an unpaired surrogate can never occur in a Rust str
        }
        if self.has(CASE_INSENSITIVE) {
            if self.has(UNICODE_CASE) {
                return format!("(?i:{})", lit(cp));
            } else if cp < 128 {
                let (lo, up) = (ascii_lower(cp), ascii_upper(cp));
                if lo != up {
                    return format!("[{}{}]", lit(lo), lit(up));
                }
            }
        }
        lit(cp)
    }

    fn emit_class(&self, cc: &Cc) -> String {
        cc.node(self.has(CASE_INSENSITIVE) && self.has(UNICODE_CASE))
    }

    /// The word-character set backing `\b` / `\B`.
    fn word_body(&self) -> String {
        if self.has(UNICODE_CHARACTER_CLASS) {
            p_word().body_text().unwrap_or_default()
        } else {
            "\\p{L}\\p{Nd}_".to_string()
        }
    }

    fn caret(&self) -> String {
        if !self.has(MULTILINE) {
            "\\A".to_string()
        } else if self.has(UNIX_LINES) {
            "(?!\\z)(?:\\A|(?<=\\x{A}))".to_string()
        } else {
            "(?!\\z)(?:\\A|(?<=[\\x{A}\\x{85}\\x{2028}\\x{2029}])|(?<=\\x{D})(?!\\x{A}))"
                .to_string()
        }
    }

    fn dollar(&self, multiline: bool) -> String {
        match (self.has(UNIX_LINES), multiline) {
            (true, true) => "(?:(?=\\x{A})|\\z)".to_string(),
            (true, false) => "(?=\\x{A}?\\z)".to_string(),
            (false, true) => {
                "(?:(?<!\\x{D})(?=\\x{A})|(?=[\\x{D}\\x{85}\\x{2028}\\x{2029}])|\\z)".to_string()
            }
            (false, false) => "(?:(?<!\\x{D})(?=\\x{A}\\z)\
                 |(?=(?:\\x{D}\\x{A}|[\\x{D}\\x{85}\\x{2028}\\x{2029}])?\\z))"
                .to_string(),
        }
    }

    fn dot(&self) -> String {
        if self.has(DOTALL) {
            "(?s:.)".to_string()
        } else if self.has(UNIX_LINES) {
            "[^\\x{A}]".to_string()
        } else {
            "[^\\x{A}\\x{D}\\x{85}\\x{2028}\\x{2029}]".to_string()
        }
    }

    // ------------------------------------------------------------- grammar

    fn expr(&mut self) -> PResult<String> {
        let mut alts: Vec<String> = Vec::new();
        loop {
            alts.push(self.sequence()?);
            if self.peek() != '|' as u32 {
                return Ok(alts.join("|"));
            }
            self.next();
        }
    }

    fn sequence(&mut self) -> PResult<String> {
        let mut parts: Vec<String> = Vec::new();
        loop {
            let ch = self.peek();
            let node: String = match ch {
                x if x == '(' as u32 => {
                    match self.group0()? {
                        None => continue, // inline modifier only
                        Some(s) => {
                            parts.push(s);
                            continue;
                        }
                    }
                }
                x if x == '[' as u32 => {
                    let cc = self.clazz(true)?;
                    self.emit_class(&cc)
                }
                x if x == '\\' as u32 => {
                    let c2 = self.next_escaped();
                    if c2 == 'p' as u32 || c2 == 'P' as u32 {
                        let comp = c2 == 'P' as u32;
                        let mut one_letter = true;
                        if self.next() != '{' as u32 {
                            self.unread();
                        } else {
                            one_letter = false;
                        }
                        let cc = self.family(one_letter, comp)?;
                        self.emit_class(&cc)
                    } else {
                        self.unread();
                        self.atom()?
                    }
                }
                x if x == '^' as u32 => {
                    self.next();
                    self.caret()
                }
                x if x == '$' as u32 => {
                    self.next();
                    self.dollar(self.has(MULTILINE))
                }
                x if x == '.' as u32 => {
                    self.next();
                    self.dot()
                }
                x if x == '|' as u32 || x == ')' as u32 => break,
                x if x == ']' as u32 || x == '}' as u32 => self.atom()?,
                x if x == '?' as u32 || x == '*' as u32 || x == '+' as u32 => {
                    self.next();
                    let c = char::from_u32(ch).unwrap_or('?');
                    return self.error(format!("Dangling meta character '{c}'"));
                }
                0 => {
                    if self.cursor >= self.pattern_length {
                        break;
                    }
                    self.atom()?
                }
                _ => self.atom()?,
            };
            let node = self.closure(node)?;
            parts.push(node);
        }
        Ok(parts.concat())
    }

    fn atom(&mut self) -> PResult<String> {
        let mut buffer: Vec<u32> = Vec::new();
        let mut prev = 0usize;
        let mut ch = self.peek();
        loop {
            let first = buffer.len();
            match ch {
                x if x == '*' as u32
                    || x == '+' as u32
                    || x == '?' as u32
                    || x == '{' as u32 =>
                {
                    if first > 1 {
                        self.cursor = prev;
                        buffer.pop();
                    }
                }
                x if x == '$' as u32
                    || x == '.' as u32
                    || x == '^' as u32
                    || x == '(' as u32
                    || x == '[' as u32
                    || x == '|' as u32
                    || x == ')' as u32 => {}
                x if x == '\\' as u32 => {
                    ch = self.next_escaped();
                    if ch == 'p' as u32 || ch == 'P' as u32 {
                        if first > 0 {
                            self.unread();
                        } else {
                            let comp = ch == 'P' as u32;
                            let mut one_letter = true;
                            if self.next() != '{' as u32 {
                                self.unread();
                            } else {
                                one_letter = false;
                            }
                            let cc = self.family(one_letter, comp)?;
                            return Ok(self.emit_class(&cc));
                        }
                    } else {
                        self.unread();
                        prev = self.cursor;
                        match self.escape(false, first == 0, false)? {
                            Esc::Ch(c) => {
                                buffer.push(c);
                                ch = self.peek();
                                continue;
                            }
                            Esc::Node(n) if first == 0 => return Ok(n),
                            _ => {
                                // Unwind meta escape sequence
                                self.cursor = prev;
                            }
                        }
                    }
                }
                0 => {
                    if self.cursor < self.pattern_length {
                        prev = self.cursor;
                        buffer.push(ch);
                        ch = self.next();
                        continue;
                    }
                }
                _ => {
                    prev = self.cursor;
                    buffer.push(ch);
                    ch = self.next();
                    continue;
                }
            }
            break;
        }
        Ok(buffer.iter().map(|&c| self.emit_single(c)).collect())
    }

    /// `Pattern.ref()` - a greedy back reference.
    fn ref_(&mut self, mut refnum: u32) -> String {
        loop {
            let ch = self.peek();
            if !(0x30..=0x39).contains(&ch) {
                break;
            }
            let new = refnum * 10 + (ch - 0x30);
            if (self.capturing_group_count as u32) - 1 < new {
                break;
            }
            refnum = new;
            self.read();
        }
        self.emit_backref(refnum as usize)
    }

    fn emit_backref(&self, group: usize) -> String {
        if group == 0 || group >= self.capturing_group_count {
            // Java allocates at least 10 group slots, so \1..\9 simply never match.
            "(?!)".to_string()
        } else if self.has(CASE_INSENSITIVE) {
            format!("(?i:\\{group})")
        } else {
            format!("\\{group}")
        }
    }

    #[allow(clippy::too_many_lines)]
    fn escape(&mut self, inclass: bool, create: bool, isrange: bool) -> PResult<Esc> {
        let ch = self.skip();
        let c = char::from_u32(ch).unwrap_or('\u{0}');
        // A predicate escape: node when outside a class, predicate when inside, nothing
        // when the caller asked for a value only.
        macro_rules! pred {
            ($cc:expr) => {{
                if !create {
                    return Ok(Esc::Nothing);
                }
                let cc = $cc;
                return Ok(if inclass {
                    Esc::Class(cc)
                } else {
                    Esc::Node(self.emit_class(&cc))
                });
            }};
        }
        match c {
            '0' => return Ok(Esc::Ch(self.octal()?)),
            '1'..='9' => {
                if !inclass {
                    if create {
                        let n = ch - '0' as u32;
                        return Ok(Esc::Node(self.ref_(n)));
                    }
                    return Ok(Esc::Nothing);
                }
            }
            'A' => {
                if !inclass {
                    return Ok(if create { Esc::Node("\\A".into()) } else { Esc::Nothing });
                }
            }
            'B' => {
                if !inclass {
                    if !create {
                        return Ok(Esc::Nothing);
                    }
                    let w = self.word_body();
                    return Ok(Esc::Node(format!("(?:(?<![{w}])(?![{w}])|(?<=[{w}])(?=[{w}]))")));
                }
            }
            'C' => {}
            'D' => pred!(self.digit_class().negate()),
            'E' | 'F' => {}
            'G' => {
                if !inclass {
                    return Ok(if create { Esc::Node("\\G".into()) } else { Esc::Nothing });
                }
            }
            'H' => pred!(horiz_ws().negate()),
            'I' | 'J' | 'K' | 'L' | 'M' => {}
            'N' => return Ok(Esc::Ch(self.char_name()?)),
            'O' | 'P' | 'Q' => {}
            'R' => {
                if !inclass {
                    if !create {
                        return Ok(Esc::Nothing);
                    }
                    return Ok(Esc::Node(
                        "(?:(?>\\x{D}\\x{A}?)|[\\x{A}\\x{B}\\x{C}\\x{85}\\x{2028}\\x{2029}])"
                            .into(),
                    ));
                }
            }
            'S' => pred!(self.space_class().negate()),
            'T' | 'U' => {}
            'V' => pred!(vert_ws().negate()),
            'W' => pred!(self.word_class().negate()),
            'X' => {
                if !inclass {
                    if !create {
                        return Ok(Esc::Nothing);
                    }
                    return self.error(UNSUPPORTED_GRAPHEME);
                }
            }
            'Y' => {}
            'Z' => {
                if !inclass {
                    return Ok(if create {
                        Esc::Node(self.dollar(false))
                    } else {
                        Esc::Nothing
                    });
                }
            }
            'a' => return Ok(Esc::Ch(0x07)),
            'b' => {
                if !inclass {
                    if !create {
                        return Ok(Esc::Nothing);
                    }
                    if self.peek() == '{' as u32 {
                        if self.skip() == 'g' as u32 {
                            if self.read() == '}' as u32 {
                                return self.error(UNSUPPORTED_GRAPHEME);
                            }
                            return self.error("Illegal/unsupported escape sequence");
                        }
                        self.unread();
                        self.unread();
                    }
                    let w = self.word_body();
                    return Ok(Esc::Node(format!(
                        "(?:(?<![{w}])(?=[{w}])|(?<=[{w}])(?![{w}]))"
                    )));
                }
            }
            'c' => return Ok(Esc::Ch(self.control()?)),
            'd' => pred!(self.digit_class()),
            'e' => return Ok(Esc::Ch(0x1B)),
            'f' => return Ok(Esc::Ch(0x0C)),
            'g' => {}
            'h' => pred!(horiz_ws()),
            'i' | 'j' => {}
            'k' => {
                if !inclass {
                    if self.read() != '<' as u32 {
                        return self
                            .error("\\k is not followed by '<' for named capturing group");
                    }
                    let r = self.read();
                    let name = self.groupname(r)?;
                    let number = self.named_groups.iter().find(|(n, _)| *n == name).map(|(_, i)| *i);
                    let Some(number) = number else {
                        return self.error(format!(
                            "named capturing group <{name}> does not exist"
                        ));
                    };
                    return Ok(if create {
                        Esc::Node(self.emit_backref(number))
                    } else {
                        Esc::Nothing
                    });
                }
            }
            'l' | 'm' => {}
            'n' => return Ok(Esc::Ch(0x0A)),
            'o' | 'p' | 'q' => {}
            'r' => return Ok(Esc::Ch(0x0D)),
            's' => pred!(self.space_class()),
            't' => return Ok(Esc::Ch(0x09)),
            'u' => return Ok(Esc::Ch(self.unicode_escape())),
            'v' => {
                if isrange {
                    return Ok(Esc::Ch(0x0B));
                }
                pred!(vert_ws())
            }
            'w' => pred!(self.word_class()),
            'x' => return Ok(Esc::Ch(self.hex_escape()?)),
            'y' => {}
            'z' => {
                if !inclass {
                    return Ok(if create { Esc::Node("\\z".into()) } else { Esc::Nothing });
                }
            }
            _ => return Ok(Esc::Ch(ch)),
        }
        self.error("Illegal/unsupported escape sequence")
    }

    fn digit_class(&self) -> Cc {
        if self.has(UNICODE_CHARACTER_CLASS) {
            p_digit()
        } else {
            Cc::set(&[(0x30, 0x39)])
        }
    }

    fn space_class(&self) -> Cc {
        if self.has(UNICODE_CHARACTER_CLASS) {
            p_white_space()
        } else {
            Cc::set(&[(0x09, 0x0D), (0x20, 0x20)])
        }
    }

    fn word_class(&self) -> Cc {
        if self.has(UNICODE_CHARACTER_CLASS) {
            p_word()
        } else {
            Cc::set(&[(0x30, 0x39), (0x41, 0x5A), (0x5F, 0x5F), (0x61, 0x7A)])
        }
    }

    fn clazz(&mut self, consume: bool) -> PResult<Cc> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            self.depth -= 1;
            return self.error(UNSUPPORTED_DEPTH);
        }
        let r = self.clazz_inner(consume);
        self.depth -= 1;
        r
    }

    fn clazz_inner(&mut self, consume: bool) -> PResult<Cc> {
        let mut prev: Option<Cc> = None;
        let mut curr: Option<Cc> = None;
        let mut is_neg = false;
        let mut ch = self.next();

        if ch == '^' as u32 && self.tget(self.cursor - 1) == '[' as u32 {
            ch = self.next();
            is_neg = true;
        }
        loop {
            if ch == '[' as u32 {
                let c = self.clazz(true)?;
                prev = Some(match prev {
                    None => c.clone(),
                    Some(p) => p.union(c.clone()),
                });
                curr = Some(c);
                ch = self.peek();
                continue;
            } else if ch == '&' as u32 {
                ch = self.next();
                if ch == '&' as u32 {
                    ch = self.next();
                    let mut right: Option<Cc> = None;
                    while ch != ']' as u32 && ch != '&' as u32 {
                        if ch == '[' as u32 {
                            let c = self.clazz(true)?;
                            right = Some(match right {
                                None => c,
                                Some(r) => r.union(c),
                            });
                        } else {
                            self.unread();
                            let c = self.clazz(false)?;
                            right = Some(match right {
                                None => c,
                                Some(r) => r.union(c),
                            });
                        }
                        ch = self.peek();
                    }
                    if let Some(r) = right.clone() {
                        curr = Some(r);
                    }
                    match prev.take() {
                        None => match right {
                            None => return self.error("Bad class syntax"),
                            Some(r) => prev = Some(r),
                        },
                        Some(p) => match curr.clone() {
                            None => return self.error("Bad intersection syntax"),
                            Some(c) => prev = Some(p.and(c)),
                        },
                    }
                    continue;
                } else {
                    // treat as a literal &
                    self.unread();
                }
            } else if ch == 0 {
                if self.cursor >= self.pattern_length {
                    return self.error("Unclosed character class");
                }
            } else if ch == ']' as u32 && prev.is_some() {
                if consume {
                    self.next();
                }
                let p = prev.take().unwrap();
                return Ok(if is_neg { p.negate() } else { p });
            }
            let c = self.range()?;
            prev = Some(match prev {
                None => c.clone(),
                Some(p) => p.union(c.clone()),
            });
            curr = Some(c);
            ch = self.peek();
        }
    }

    fn range(&mut self) -> PResult<Cc> {
        let mut ch = self.peek();
        if ch == '\\' as u32 {
            ch = self.next_escaped();
            if ch == 'p' as u32 || ch == 'P' as u32 {
                let comp = ch == 'P' as u32;
                let mut one_letter = true;
                if self.next() != '{' as u32 {
                    self.unread();
                } else {
                    one_letter = false;
                }
                return self.family(one_letter, comp);
            }
            let isrange = self.tget(self.cursor + 1) == '-' as u32;
            self.unread();
            match self.escape(true, true, isrange)? {
                Esc::Class(cc) => return Ok(cc),
                Esc::Ch(c) => ch = c,
                _ => return self.error("Illegal character range"),
            }
        } else {
            self.next();
        }
        if self.peek() == '-' as u32 {
            let end_range = self.tget(self.cursor + 1);
            if end_range == '[' as u32 {
                return Ok(self.single_class(ch));
            }
            if end_range != ']' as u32 {
                self.next();
                let mut m = self.peek();
                if m == '\\' as u32 {
                    match self.escape(true, false, true)? {
                        Esc::Ch(c) => m = c,
                        // Java's -1 compares below every code point.
                        _ => return self.error("Illegal character range"),
                    }
                } else {
                    self.next();
                }
                if m < ch {
                    return self.error("Illegal character range");
                }
                return Ok(if self.has(CASE_INSENSITIVE) && !self.has(UNICODE_CASE) {
                    Cc::Set(ascii_ci_ranges(ch, m))
                } else {
                    Cc::set(&[(ch, m)])
                });
            }
        }
        Ok(self.single_class(ch))
    }

    /// `bitsOrSingle`/`single` for a class member.
    fn single_class(&self, cp: u32) -> Cc {
        if self.has(CASE_INSENSITIVE) && !self.has(UNICODE_CASE) && cp < 128 {
            let (lo, up) = (ascii_lower(cp), ascii_upper(cp));
            if lo != up {
                return Cc::set(&[(lo, lo), (up, up)]);
            }
        }
        Cc::set(&[(cp, cp)])
    }

    fn family(&mut self, single_letter: bool, is_complement: bool) -> PResult<Cc> {
        self.next();
        let name: String;
        if single_letter {
            let c = self.tget(self.cursor);
            name = char::from_u32(c).map(String::from).unwrap_or_default();
            self.read();
        } else {
            let i = self.cursor;
            self.mark('}' as u32);
            while self.read() != '}' as u32 {
                if self.cursor > self.pattern_length + 1 {
                    break;
                }
            }
            self.mark(0);
            let j = self.cursor;
            if j > self.pattern_length {
                return self.error("Unclosed character family");
            }
            if i + 1 >= j {
                return self.error("Empty character family");
            }
            name = self.temp[i..j - 1].iter().filter_map(|&c| char::from_u32(c)).collect();
        }

        let ci = self.has(CASE_INSENSITIVE);
        let p = if let Some(i) = name.find('=') {
            let value = &name[i + 1..];
            let key = name[..i].to_lowercase();
            let p = match key.as_str() {
                "sc" | "script" => for_unicode_script(value),
                "blk" | "block" => for_unicode_block(value),
                "gc" | "general_category" => for_property(value, ci),
                _ => None,
            };
            match p {
                Some(p) => p,
                None => {
                    return self.error(format!(
                        "Unknown Unicode property {{name=<{key}>, value=<{value}>}}"
                    ))
                }
            }
        } else {
            let p = if let Some(rest) = name.strip_prefix("In") {
                for_unicode_block(rest)
            } else if let Some(short) = name.strip_prefix("Is") {
                unicode_predicate(&short.to_uppercase(), ci)
                    .or_else(|| posix_predicate(&short.to_uppercase(), ci))
                    .or_else(|| for_property(short, ci))
                    .or_else(|| for_unicode_script(short))
            } else {
                let mut p = None;
                if self.has(UNICODE_CHARACTER_CLASS) {
                    p = posix_predicate(&name.to_uppercase(), ci);
                }
                p.or_else(|| for_property(&name, ci))
            };
            match p {
                Some(p) => p,
                None => {
                    if UNSUPPORTED_PROPERTIES.contains(&name.as_str()) {
                        return self.error(format!("{UNSUPPORTED_PROPERTY}: {name}"));
                    }
                    return self.error(format!("Unknown character property name {{{name}}}"));
                }
            }
        };
        Ok(if is_complement { p.negate() } else { p })
    }

    fn groupname(&mut self, ch: u32) -> PResult<String> {
        let mut sb = String::new();
        if !ascii_is_alpha(ch) {
            return self.error("capturing group name does not start with a Latin letter");
        }
        let mut ch = ch;
        loop {
            if let Some(c) = char::from_u32(ch) {
                sb.push(c);
            }
            ch = self.read();
            if !ascii_is_alnum(ch) {
                break;
            }
        }
        if ch != '>' as u32 {
            return self.error("named capturing group is missing trailing '>'");
        }
        Ok(sb)
    }

    fn group0(&mut self) -> PResult<Option<String>> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            self.depth -= 1;
            return self.error(UNSUPPORTED_DEPTH);
        }
        let r = self.group0_inner();
        self.depth -= 1;
        r
    }

    fn group0_inner(&mut self) -> PResult<Option<String>> {
        let save = self.flags0;
        let mut ch = self.next();
        let head: String;
        if ch == '?' as u32 {
            ch = self.skip();
            let c = char::from_u32(ch).unwrap_or('\u{0}');
            match c {
                ':' => head = format!("(?:{})", self.expr()?),
                '=' | '!' => {
                    let body = self.expr()?;
                    head = format!("(?{}{})", if c == '=' { '=' } else { '!' }, wrap(&body));
                }
                '>' => head = format!("(?>{})", self.expr()?),
                '<' => {
                    ch = self.read();
                    if ch != '=' as u32 && ch != '!' as u32 {
                        let name = self.groupname(ch)?;
                        if self.named_groups.iter().any(|(n, _)| *n == name) {
                            return self
                                .error(format!("Named capturing group <{name}> is already defined"));
                        }
                        let index = self.capturing_group_count;
                        self.capturing_group_count += 1;
                        self.named_groups.push((name, index));
                        head = format!("({})", self.expr()?);
                    } else {
                        let positive = ch == '=' as u32;
                        let body = self.expr()?;
                        // fancy-regex needs each look-behind to be constant width; a
                        // top-level alternation distributes exactly.
                        let alts = split_alternation(&body);
                        head = if positive {
                            format!(
                                "(?:{})",
                                alts.iter()
                                    .map(|a| format!("(?<={a})"))
                                    .collect::<Vec<_>>()
                                    .join("|")
                            )
                        } else {
                            alts.iter().map(|a| format!("(?<!{a})")).collect::<Vec<_>>().join("")
                        };
                    }
                }
                '$' | '@' => return self.error("Unknown group type"),
                _ => {
                    self.unread();
                    self.add_flag();
                    ch = self.read();
                    if ch == ')' as u32 {
                        return Ok(None);
                    }
                    if ch != ':' as u32 {
                        return self.error("Unknown inline modifier");
                    }
                    head = format!("(?:{})", self.expr()?);
                }
            }
        } else {
            self.capturing_group_count += 1;
            head = format!("({})", self.expr()?);
        }
        self.accept(')' as u32, "Unclosed group")?;
        self.flags0 = save;
        Ok(Some(self.closure(head)?))
    }

    fn add_flag(&mut self) {
        let mut ch = self.peek();
        loop {
            match char::from_u32(ch).unwrap_or('\u{0}') {
                'i' => self.flags0 |= CASE_INSENSITIVE,
                'm' => self.flags0 |= MULTILINE,
                's' => self.flags0 |= DOTALL,
                'd' => self.flags0 |= UNIX_LINES,
                'u' => self.flags0 |= UNICODE_CASE,
                'c' => self.flags0 |= CANON_EQ,
                'x' => self.flags0 |= COMMENTS,
                'U' => self.flags0 |= UNICODE_CHARACTER_CLASS | UNICODE_CASE,
                '-' => {
                    self.next();
                    self.sub_flag();
                    return;
                }
                _ => return,
            }
            ch = self.next();
        }
    }

    fn sub_flag(&mut self) {
        let mut ch = self.peek();
        loop {
            match char::from_u32(ch).unwrap_or('\u{0}') {
                'i' => self.flags0 &= !CASE_INSENSITIVE,
                'm' => self.flags0 &= !MULTILINE,
                's' => self.flags0 &= !DOTALL,
                'd' => self.flags0 &= !UNIX_LINES,
                'u' => self.flags0 &= !UNICODE_CASE,
                'c' => self.flags0 &= !CANON_EQ,
                'x' => self.flags0 &= !COMMENTS,
                'U' => self.flags0 &= !(UNICODE_CHARACTER_CLASS | UNICODE_CASE),
                _ => return,
            }
            ch = self.next();
        }
    }

    /// `Pattern.qtype()`; returns the suffix to append to a greedy quantifier.
    fn qtype(&mut self) -> &'static str {
        let ch = self.next();
        if ch == '?' as u32 {
            self.next();
            "?"
        } else if ch == '+' as u32 {
            self.next();
            "+"
        } else {
            ""
        }
    }

    fn closure(&mut self, prev: String) -> PResult<String> {
        let ch = self.peek();
        let c = char::from_u32(ch).unwrap_or('\u{0}');
        match c {
            '?' | '*' | '+' => {
                let q = self.qtype();
                Ok(format!("{}{c}{q}", wrap(&prev)))
            }
            '{' => {
                let mut ch = self.skip();
                if !ascii_is_digit(ch) {
                    return self.error("Illegal repetition");
                }
                let mut cmin: i32 = 0;
                let cmax: i32;
                loop {
                    cmin = match cmin.checked_mul(10).and_then(|v| v.checked_add((ch - 0x30) as i32))
                    {
                        Some(v) => v,
                        None => return self.error("Illegal repetition range"),
                    };
                    ch = self.read();
                    if !ascii_is_digit(ch) {
                        break;
                    }
                }
                let mut open_ended = false;
                if ch == ',' as u32 {
                    ch = self.read();
                    if ch == '}' as u32 {
                        self.unread();
                        open_ended = true;
                        cmax = i32::MAX;
                    } else {
                        let mut v: i32 = 0;
                        while ascii_is_digit(ch) {
                            v = match v.checked_mul(10).and_then(|x| x.checked_add((ch - 0x30) as i32))
                            {
                                Some(v) => v,
                                None => return self.error("Illegal repetition range"),
                            };
                            ch = self.read();
                        }
                        cmax = v;
                    }
                } else {
                    cmax = cmin;
                }
                if !open_ended {
                    if ch != '}' as u32 {
                        return self.error("Unclosed counted closure");
                    }
                    if cmax < cmin {
                        return self.error("Illegal repetition range");
                    }
                    self.unread();
                }
                let q = self.qtype();
                Ok(if open_ended {
                    format!("{}{{{cmin},}}{q}", wrap(&prev))
                } else if cmin == cmax {
                    format!("{}{{{cmin}}}{q}", wrap(&prev))
                } else {
                    format!("{}{{{cmin},{cmax}}}{q}", wrap(&prev))
                })
            }
            _ => Ok(prev),
        }
    }

    // ------------------------------------------------------------- escape values

    fn control(&mut self) -> PResult<u32> {
        if self.cursor < self.pattern_length {
            return Ok(self.read() ^ 64);
        }
        self.error("Illegal control escape sequence")
    }

    fn octal(&mut self) -> PResult<u32> {
        let n = self.read();
        if is_octal(n) {
            let m = self.read();
            if is_octal(m) {
                let o = self.read();
                if is_octal(o) && (0x30..=0x33).contains(&n) {
                    return Ok((n - 0x30) * 64 + (m - 0x30) * 8 + (o - 0x30));
                }
                self.unread();
                return Ok((n - 0x30) * 8 + (m - 0x30));
            }
            self.unread();
            return Ok(n - 0x30);
        }
        self.error("Illegal octal escape sequence")
    }

    fn hex_escape(&mut self) -> PResult<u32> {
        let n = self.read();
        if ascii_is_hex(n) {
            let m = self.read();
            if ascii_is_hex(m) {
                return Ok(to_digit(n) * 16 + to_digit(m));
            }
        } else if n == '{' as u32 && ascii_is_hex(self.peek()) {
            let mut cp: u32 = 0;
            let mut n = self.read();
            while ascii_is_hex(n) {
                cp = cp.saturating_mul(16).saturating_add(to_digit(n));
                if cp > 0x10_FFFF {
                    return self.error("Hexadecimal codepoint is too big");
                }
                n = self.read();
            }
            if n != '}' as u32 {
                return self.error("Unclosed hexadecimal escape sequence");
            }
            return Ok(cp);
        }
        self.error("Illegal hexadecimal escape sequence")
    }

    fn uxxxx(&mut self) -> PResult<u32> {
        let mut n = 0u32;
        for _ in 0..4 {
            let ch = self.read();
            if !ascii_is_hex(ch) {
                return self.error("Illegal Unicode escape sequence");
            }
            n = n * 16 + to_digit(ch);
        }
        Ok(n)
    }

    /// `Pattern.u()`; errors are raised eagerly like the JDK's.
    fn unicode_escape(&mut self) -> u32 {
        match self.u_inner() {
            Ok(v) => v,
            Err(e) => {
                self.pending_error = Some(e);
                0
            }
        }
    }

    fn u_inner(&mut self) -> PResult<u32> {
        let n = self.uxxxx()?;
        if (0xD800..=0xDBFF).contains(&n) {
            let cur = self.cursor;
            if self.read() == '\\' as u32 && self.read() == 'u' as u32 {
                if let Ok(n2) = self.uxxxx() {
                    if (0xDC00..=0xDFFF).contains(&n2) {
                        return Ok(0x10000 + ((n - 0xD800) << 10) + (n2 - 0xDC00));
                    }
                }
            }
            self.cursor = cur;
        }
        Ok(n)
    }

    fn char_name(&mut self) -> PResult<u32> {
        if self.read() == '{' as u32 {
            while self.read() != '}' as u32 {
                if self.cursor >= self.pattern_length {
                    return self.error("Unclosed character name escape sequence");
                }
            }
            return self.error(UNSUPPORTED_CHAR_NAME);
        }
        self.error("Illegal character name escape sequence")
    }
}

const UNSUPPORTED_GRAPHEME: &str = "Unsupported: grapheme cluster matching (\\X, \\b{g})";
const UNSUPPORTED_CHAR_NAME: &str = "Unsupported: \\N{name} needs the Unicode name table";
const UNSUPPORTED_PROPERTY: &str = "Unsupported character property";
const UNSUPPORTED_DEPTH: &str = "Unsupported: pattern nests too deeply";
const UNSUPPORTED_CANON_EQ: &str = "Unsupported: CANON_EQ needs Unicode normalisation";
/// Property names the JDK knows but regex-syntax cannot express.
const UNSUPPORTED_PROPERTIES: &[&str] = &["javaMirrored"];

fn ascii_is_space(ch: u32) -> bool {
    ch == 0x20 || (0x09..=0x0D).contains(&ch)
}
fn ascii_is_digit(ch: u32) -> bool {
    (0x30..=0x39).contains(&ch)
}
fn ascii_is_alpha(ch: u32) -> bool {
    (0x41..=0x5A).contains(&ch) || (0x61..=0x7A).contains(&ch)
}
fn ascii_is_alnum(ch: u32) -> bool {
    ascii_is_alpha(ch) || ascii_is_digit(ch)
}
fn ascii_is_hex(ch: u32) -> bool {
    ascii_is_digit(ch) || (0x41..=0x46).contains(&ch) || (0x61..=0x66).contains(&ch)
}
fn to_digit(ch: u32) -> u32 {
    if ascii_is_digit(ch) { ch - 0x30 } else { (ch | 0x20) - 0x61 + 10 }
}
fn is_octal(ch: u32) -> bool {
    (0x30..=0x37).contains(&ch)
}

fn horiz_ws() -> Cc {
    Cc::set(&[
        (0x09, 0x09),
        (0x20, 0x20),
        (0xA0, 0xA0),
        (0x1680, 0x1680),
        (0x180E, 0x180E),
        (0x2000, 0x200A),
        (0x202F, 0x202F),
        (0x205F, 0x205F),
        (0x3000, 0x3000),
    ])
}

fn vert_ws() -> Cc {
    Cc::set(&[(0x0A, 0x0D), (0x85, 0x85), (0x2028, 0x2029)])
}

/// Wraps a node so a quantifier binds to all of it (and so fancy-regex accepts quantified
/// assertions, which Java allows).
fn wrap(node: &str) -> String {
    format!("(?:{node})")
}

/// Splits an emitted alternation at its top level (parenthesis- and class-aware).
fn split_alternation(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut in_class = false;
    let mut start = 0usize;
    let b = body.as_bytes();
    let mut i = 0usize;
    while i < b.len() {
        match b[i] {
            b'\\' => i += 1,
            b'[' if !in_class => in_class = true,
            b']' if in_class => in_class = false,
            b'(' if !in_class => depth += 1,
            b')' if !in_class => depth -= 1,
            b'|' if !in_class && depth == 0 => {
                out.push(body[start..i].to_string());
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    out.push(body[start..].to_string());
    out
}

// --------------------------------------------------------------------- Pattern

#[derive(Clone, Debug)]
pub struct Pattern {
    src: String,
    flags: i32,
    translated: String,
    re: Regex,
    re_full: Regex,
    group_count: usize,
    named_groups: Vec<(String, usize)>,
}

impl fmt::Display for Pattern {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.src)
    }
}

impl Pattern {
    pub fn compile(regex: &str) -> PResult<Pattern> {
        Pattern::compile_flags(regex, 0)
    }

    /// Unknown flag bits are rejected like `new Pattern(String,int)` does.
    pub fn compile_flags_checked(regex: &str, flags: i32) -> Result<Pattern, JavaRegexError> {
        if flags & !ALL_FLAGS != 0 {
            return Err(JavaRegexError::IllegalArgument(format!(
                "Unknown flag 0x{:x}",
                flags as u32
            )));
        }
        Pattern::compile_flags(regex, flags).map_err(JavaRegexError::Syntax)
    }

    pub fn compile_flags(regex: &str, flags: i32) -> PResult<Pattern> {
        if flags & CANON_EQ != 0 && flags & LITERAL == 0 {
            return Err(PatternSyntaxException::new(UNSUPPORTED_CANON_EQ, regex, -1));
        }
        let translated = translate(regex, flags)?;
        let build = |src: &str| {
            fancy_regex::RegexBuilder::new(src).backtrack_limit(BACKTRACK_LIMIT).build()
        };
        let re = build(&translated.text).map_err(|e| {
            PatternSyntaxException::new(format!("Unsupported by fancy-regex: {e}"), regex, -1)
        })?;
        let re_full = build(&format!("\\A(?:{})\\z", translated.text)).map_err(|e| {
            PatternSyntaxException::new(format!("Unsupported by fancy-regex: {e}"), regex, -1)
        })?;
        Ok(Pattern {
            src: regex.to_string(),
            flags,
            translated: translated.text,
            re,
            re_full,
            group_count: translated.group_count,
            named_groups: translated.named_groups,
        })
    }

    pub fn pattern(&self) -> &str {
        &self.src
    }

    pub fn flags(&self) -> i32 {
        self.flags
    }

    /// The fancy-regex source this pattern was translated to (for diagnostics and tests).
    pub fn translated(&self) -> &str {
        &self.translated
    }

    /// `Matcher.matches()` - the whole input must match.
    pub fn matches(&self, input: &str) -> bool {
        self.re_full.is_match(input).unwrap_or(false)
    }

    /// `Matcher.find()` - any subsequence.
    pub fn find(&self, input: &str) -> bool {
        self.re.is_match(input).unwrap_or(false)
    }

    /// The successive `Matcher.find()` positions, with Java's "advance one character past an
    /// empty match" rule.
    fn find_all(&self, input: &str) -> Vec<Vec<Option<(usize, usize)>>> {
        let mut out = Vec::new();
        let mut pos = 0usize;
        loop {
            let caps = match self.re.captures_from_pos(input, pos) {
                Ok(Some(c)) => c,
                _ => break,
            };
            let mut groups: Vec<Option<(usize, usize)>> = Vec::with_capacity(self.group_count + 1);
            for i in 0..=self.group_count {
                groups.push(caps.get(i).map(|m| (m.start(), m.end())));
            }
            let (s, e) = groups[0].expect("group 0 always participates");
            out.push(groups);
            if e == s {
                if e >= input.len() {
                    break;
                }
                pos = e + input[e..].chars().next().map_or(1, char::len_utf8);
            } else {
                pos = e;
            }
        }
        out
    }

    /// `Pattern.split(CharSequence, int)`.
    pub fn split(&self, input: &str, limit: i32) -> Vec<String> {
        let mut match_count: i32 = 0;
        let mut index = 0usize;
        let match_limited = limit > 0;
        let mut list: Vec<String> = Vec::new();
        for g in self.find_all(input) {
            let (s, e) = g[0].unwrap();
            if !match_limited || match_count < limit - 1 {
                if index == 0 && index == s && s == e {
                    continue;
                }
                list.push(input[index..s].to_string());
                index = e;
                match_count += 1;
            } else if match_count == limit - 1 {
                list.push(input[index..].to_string());
                index = e;
                match_count += 1;
            }
        }
        if index == 0 {
            return vec![input.to_string()];
        }
        if !match_limited || match_count < limit {
            list.push(input[index..].to_string());
        }
        let mut n = list.len();
        if limit == 0 {
            while n > 0 && list[n - 1].is_empty() {
                n -= 1;
            }
        }
        list.truncate(n);
        list
    }

    pub fn replace_all(&self, input: &str, replacement: &str) -> Result<String, JavaRegexError> {
        self.replace(input, replacement, usize::MAX)
    }

    pub fn replace_first(&self, input: &str, replacement: &str) -> Result<String, JavaRegexError> {
        self.replace(input, replacement, 1)
    }

    fn replace(
        &self,
        input: &str,
        replacement: &str,
        max: usize,
    ) -> Result<String, JavaRegexError> {
        let all = self.find_all(input);
        if all.is_empty() {
            return Ok(input.to_string());
        }
        let mut sb = String::new();
        let mut last_append = 0usize;
        for g in all.iter().take(max) {
            let (s, e) = g[0].unwrap();
            sb.push_str(&input[last_append..s]);
            self.append_expanded(&mut sb, replacement, g, input)?;
            last_append = e;
        }
        sb.push_str(&input[last_append..]);
        Ok(sb)
    }

    /// `Matcher.appendExpandedReplacement`.
    fn append_expanded(
        &self,
        out: &mut String,
        replacement: &str,
        groups: &[Option<(usize, usize)>],
        input: &str,
    ) -> Result<(), JavaRegexError> {
        let r: Vec<char> = replacement.chars().collect();
        let mut cursor = 0usize;
        while cursor < r.len() {
            let next_char = r[cursor];
            if next_char == '\\' {
                cursor += 1;
                if cursor == r.len() {
                    return Err(JavaRegexError::IllegalArgument(
                        "character to be escaped is missing".into(),
                    ));
                }
                out.push(r[cursor]);
                cursor += 1;
            } else if next_char == '$' {
                cursor += 1;
                if cursor == r.len() {
                    return Err(JavaRegexError::IllegalArgument(
                        "Illegal group reference: group index is missing".into(),
                    ));
                }
                let mut next_char = r[cursor];
                let refnum: usize;
                if next_char == '{' {
                    cursor += 1;
                    let begin = cursor;
                    while cursor < r.len() {
                        next_char = r[cursor];
                        if next_char.is_ascii_alphanumeric() {
                            cursor += 1;
                        } else {
                            break;
                        }
                    }
                    if begin == cursor {
                        return Err(JavaRegexError::IllegalArgument(
                            "named capturing group has 0 length name".into(),
                        ));
                    }
                    if next_char != '}' {
                        return Err(JavaRegexError::IllegalArgument(
                            "named capturing group is missing trailing '}'".into(),
                        ));
                    }
                    let gname: String = r[begin..cursor].iter().collect();
                    if gname.starts_with(|c: char| c.is_ascii_digit()) {
                        return Err(JavaRegexError::IllegalArgument(format!(
                            "capturing group name {{{gname}}} starts with digit character"
                        )));
                    }
                    match self.named_groups.iter().find(|(n, _)| *n == gname) {
                        Some((_, i)) => refnum = *i,
                        None => {
                            return Err(JavaRegexError::IllegalArgument(format!(
                                "No group with name {{{gname}}}"
                            )))
                        }
                    }
                    cursor += 1;
                } else {
                    let d = next_char as i32 - '0' as i32;
                    if !(0..=9).contains(&d) {
                        return Err(JavaRegexError::IllegalArgument(
                            "Illegal group reference".into(),
                        ));
                    }
                    let mut n = d as usize;
                    cursor += 1;
                    loop {
                        if cursor >= r.len() {
                            break;
                        }
                        let nd = r[cursor] as i32 - '0' as i32;
                        if !(0..=9).contains(&nd) {
                            break;
                        }
                        let new = n * 10 + nd as usize;
                        if self.group_count < new {
                            break;
                        }
                        n = new;
                        cursor += 1;
                    }
                    refnum = n;
                }
                if refnum > self.group_count {
                    return Err(JavaRegexError::IndexOutOfBounds(format!("No group {refnum}")));
                }
                if let Some((s, e)) = groups[refnum] {
                    out.push_str(&input[s..e]);
                }
            } else {
                out.push(next_char);
                cursor += 1;
            }
        }
        Ok(())
    }
}

// --------------------------------------------------------------------- translation

struct Translated {
    text: String,
    group_count: usize,
    named_groups: Vec<(String, usize)>,
}

/// `Pattern.compile()`: `\Q` removal, then recursive-descent parsing into fancy-regex text.
fn translate(regex: &str, flags: i32) -> PResult<Translated> {
    let mut temp: Vec<u32> = regex.chars().map(|c| c as u32).collect();
    let mut pattern_length = temp.len();
    temp.push(0);
    temp.push(0);

    if flags & LITERAL == 0 {
        remove_qe_quoting(&mut temp, &mut pattern_length);
    }

    if flags & LITERAL != 0 {
        let p = Parser {
            original: regex,
            temp: temp.clone(),
            pattern_length,
            cursor: 0,
            flags0: flags,
            capturing_group_count: 1,
            named_groups: Vec::new(),
            depth: 0,
            pending_error: None,
        };
        let text: String = temp[..pattern_length].iter().map(|&c| p.emit_single(c)).collect();
        return Ok(Translated { text, group_count: 0, named_groups: Vec::new() });
    }

    let mut p = Parser {
        original: regex,
        temp,
        pattern_length,
        cursor: 0,
        flags0: flags,
        capturing_group_count: 1,
        named_groups: Vec::new(),
        depth: 0,
        pending_error: None,
    };
    let text = p.expr()?;
    if let Some(e) = p.pending_error.take() {
        return Err(e);
    }
    if p.pattern_length != p.cursor {
        if p.peek() == ')' as u32 {
            return p.error("Unmatched closing ')'");
        } else if p.cursor == p.pattern_length + 1
            && p.pattern_length > 0
            && p.tget(p.pattern_length - 1) == '\\' as u32
        {
            return p.error("Unescaped trailing backslash");
        }
        return p.error("Unexpected internal error");
    }
    Ok(Translated {
        text,
        group_count: p.capturing_group_count - 1,
        named_groups: p.named_groups,
    })
}

/// `Pattern.RemoveQEQuoting`.
fn remove_qe_quoting(temp: &mut Vec<u32>, pattern_length: &mut usize) {
    let p_len = *pattern_length;
    let mut i = 0usize;
    while i + 1 < p_len {
        if temp[i] != '\\' as u32 {
            i += 1;
        } else if temp[i + 1] != 'Q' as u32 {
            i += 2;
        } else {
            break;
        }
    }
    if i + 1 >= p_len {
        return;
    }
    let mut j = i;
    i += 2;
    let mut newtemp: Vec<u32> = vec![0; j + 2 + 3 * (p_len - i)];
    newtemp[..j].copy_from_slice(&temp[..j]);

    let mut in_quote = true;
    let mut begin_quote = true;
    while i < p_len {
        let c = temp[i];
        i += 1;
        if c > 127 || ascii_is_alpha(c) {
            newtemp[j] = c;
            j += 1;
        } else if ascii_is_digit(c) {
            if begin_quote {
                newtemp[j] = '\\' as u32;
                newtemp[j + 1] = 'x' as u32;
                newtemp[j + 2] = '3' as u32;
                j += 3;
            }
            newtemp[j] = c;
            j += 1;
        } else if c != '\\' as u32 {
            if in_quote {
                newtemp[j] = '\\' as u32;
                j += 1;
            }
            newtemp[j] = c;
            j += 1;
        } else if in_quote {
            if temp[i] == 'E' as u32 {
                i += 1;
                in_quote = false;
            } else {
                newtemp[j] = '\\' as u32;
                newtemp[j + 1] = '\\' as u32;
                j += 2;
            }
        } else if temp[i] == 'Q' as u32 {
            i += 1;
            in_quote = true;
            begin_quote = true;
            continue;
        } else {
            newtemp[j] = c;
            j += 1;
            if i != p_len {
                newtemp[j] = temp[i];
                i += 1;
                j += 1;
            }
        }
        begin_quote = false;
    }
    *pattern_length = j;
    newtemp.truncate(j);
    newtemp.push(0);
    newtemp.push(0);
    *temp = newtemp;
}

// --------------------------------------------------------------------- String shims

/// `String.matches(String)`.
pub fn string_matches(input: &str, regex: &str) -> PResult<bool> {
    Ok(Pattern::compile(regex)?.matches(input))
}

/// `String.split(String, int)`, including the single-character fast path that bypasses
/// `Pattern.compile` entirely (so an otherwise invalid regex may not even be parsed).
pub fn string_split(input: &str, regex: &str, limit: i32) -> PResult<Vec<String>> {
    if let Some(ch) = split_fastpath_char(regex) {
        return Ok(split_on_char(input, ch, limit));
    }
    Ok(Pattern::compile(regex)?.split(input, limit))
}

fn split_fastpath_char(regex: &str) -> Option<char> {
    let u: Vec<u16> = regex.encode_utf16().collect();
    let ch = if u.len() == 1 && !".$|()[{^?*+\\".encode_utf16().any(|m| m == u[0]) {
        u[0]
    } else if u.len() == 2 && u[0] == '\\' as u16 {
        let c = u[1];
        let is_alnum = (0x30..=0x39).contains(&c)
            || (0x61..=0x7A).contains(&c)
            || (0x41..=0x5A).contains(&c);
        if is_alnum {
            return None;
        }
        c
    } else {
        return None;
    };
    if (0xD800..=0xDFFF).contains(&ch) {
        return None;
    }
    char::from_u32(ch as u32)
}

fn split_on_char(input: &str, ch: char, limit: i32) -> Vec<String> {
    let mut match_count = 0i32;
    let mut off = 0usize;
    let limited = limit > 0;
    let mut list: Vec<String> = Vec::new();
    while let Some(rel) = input[off..].find(ch) {
        let next = off + rel;
        if !limited || match_count < limit - 1 {
            list.push(input[off..next].to_string());
            off = next + ch.len_utf8();
            match_count += 1;
        } else {
            list.push(input[off..].to_string());
            off = input.len();
            match_count += 1;
            break;
        }
    }
    if off == 0 {
        return vec![input.to_string()];
    }
    if !limited || match_count < limit {
        list.push(input[off..].to_string());
    }
    let mut n = list.len();
    if limit == 0 {
        while n > 0 && list[n - 1].is_empty() {
            n -= 1;
        }
    }
    list.truncate(n);
    list
}

/// `String.replaceAll(String, String)`.
pub fn string_replace_all(
    input: &str,
    regex: &str,
    replacement: &str,
) -> Result<String, JavaRegexError> {
    Pattern::compile(regex)?.replace_all(input, replacement)
}

/// `String.replaceFirst(String, String)`.
pub fn string_replace_first(
    input: &str,
    regex: &str,
    replacement: &str,
) -> Result<String, JavaRegexError> {
    Pattern::compile(regex)?.replace_first(input, replacement)
}

/// `Pattern.quote`.
pub fn quote(s: &str) -> String {
    if !s.contains("\\E") {
        return format!("\\Q{s}\\E");
    }
    let mut out = String::from("\\Q");
    let mut cur = 0usize;
    while let Some(rel) = s[cur..].find("\\E") {
        let i = cur + rel;
        out.push_str(&s[cur..i]);
        out.push_str("\\E\\\\E\\Q");
        cur = i + 2;
    }
    out.push_str(&s[cur..]);
    out.push_str("\\E");
    out
}

/// `Matcher.quoteReplacement`.
pub fn quote_replacement(s: &str) -> String {
    if !s.contains('\\') && !s.contains('$') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len() * 2);
    for c in s.chars() {
        if c == '\\' || c == '$' {
            out.push('\\');
        }
        out.push(c);
    }
    out
}
