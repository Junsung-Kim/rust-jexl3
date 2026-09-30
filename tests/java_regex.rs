// port of: (tests) java.util.regex.Pattern / Matcher / PatternSyntaxException,
// plus String.matches/split/replaceAll/replaceFirst.
// Replays JVM-generated fixtures from tools/javagen/RegexGen.java (Corretto 25).
// Set JAVA_REGEX_DIR to replay a locally generated campaign instead of tests/data/java_regex.
// Set JAVA_REGEX_FULL=1 to check \p{...} over every code point instead of a sample.
use rust_jexl::java::regex::*;
use std::path::PathBuf;

fn data_dir() -> PathBuf {
    match std::env::var("JAVA_REGEX_DIR") {
        Ok(d) => PathBuf::from(d),
        Err(_) => PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data/java_regex"),
    }
}

fn lines(name: &str) -> Vec<String> {
    let p = data_dir().join(name);
    let s = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
    s.lines().map(str::to_owned).collect()
}

/// Inverse of RegexGen.esc: `\\` and `\uXXXX` (UTF-16 code units).
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

struct Check {
    name: &'static str,
    count: usize,
    failures: Vec<String>,
}

impl Check {
    fn new(name: &'static str) -> Self {
        Check { name, count: 0, failures: Vec::new() }
    }
    fn eq(&mut self, what: &str, got: &str, want: &str) {
        self.count += 1;
        if got != want && self.failures.len() < 40 {
            self.failures.push(format!("{what}: got {got:?} want {want:?}"));
        } else if got != want {
            self.failures.push(String::new());
        }
    }
    fn done(self) {
        let bad = self.failures.iter().filter(|f| !f.is_empty()).count();
        let total = self.failures.len();
        if total > 0 {
            for f in self.failures.iter().filter(|f| !f.is_empty()) {
                eprintln!("  {f}");
            }
            panic!("{}: {total} of {} cases differ (showing {bad})", self.name, self.count);
        }
        eprintln!("{}: {} cases OK", self.name, self.count);
    }
}

// --------------------------------------------------------------- encodings

/// Renders a compile result the way RegexGen.compileCase does (fields after the pattern).
fn enc_compile(r: &Result<Pattern, PatternSyntaxException>, src: &str) -> String {
    match r {
        Ok(p) => {
            assert_eq!(p.pattern(), src);
            assert_eq!(p.to_string(), src);
            "OK".to_string()
        }
        Err(e) => format!("P\t{}\t{}\t{}", e.get_index(), e.get_description(), e.get_message()),
    }
}

fn enc_err(e: &JavaRegexError) -> String {
    match e {
        JavaRegexError::Syntax(p) => format!("EX:P:{}:{}", p.get_index(), p.get_description()),
        JavaRegexError::IllegalArgument(m) => format!("EX:I:{m}"),
        JavaRegexError::IndexOutOfBounds(m) => format!("EX:X:{m}"),
    }
}

fn enc_str(r: Result<String, JavaRegexError>) -> String {
    match r {
        Ok(s) => s,
        Err(e) => enc_err(&e),
    }
}

fn enc_arr(v: &[String]) -> String {
    let mut s = v.len().to_string();
    for item in v {
        s.push('\u{1}');
        s.push_str(item);
    }
    s
}

fn enc_arr_res(r: Result<Vec<String>, PatternSyntaxException>) -> String {
    match r {
        Ok(v) => enc_arr(&v),
        Err(e) => format!("EX:P:{}:{}", e.get_index(), e.get_description()),
    }
}

fn enc_bool_res(r: Result<bool, PatternSyntaxException>) -> String {
    match r {
        Ok(b) => bool_str(b),
        Err(e) => format!("EX:P:{}:{}", e.get_index(), e.get_description()),
    }
}

fn bool_str(b: bool) -> String {
    if b { "T".into() } else { "F".into() }
}

/// Decodes the expected value of an array/exception field into the same shape.
fn want_field(raw: &str) -> String {
    if let Some(rest) = raw.strip_prefix("EX:P:") {
        let (idx, desc) = rest.split_once(':').unwrap();
        format!("EX:P:{idx}:{}", unesc(desc))
    } else if let Some(m) = raw.strip_prefix("EX:I:") {
        format!("EX:I:{}", unesc(m))
    } else if let Some(m) = raw.strip_prefix("EX:X:") {
        format!("EX:X:{}", unesc(m))
    } else {
        unesc(raw)
    }
}

/// Decodes an expected string-array field (`count` then U+0001-separated elements).
fn want_arr(raw: &str) -> String {
    if raw.starts_with("EX:") {
        return want_field(raw);
    }
    let mut it = raw.split('\u{1}');
    let n: usize = it.next().unwrap().parse().unwrap();
    let items: Vec<String> = it.map(unesc).collect();
    assert_eq!(items.len(), n, "bad array field {raw:?}");
    enc_arr(&items)
}

// --------------------------------------------------------------- tests

#[test]
fn compile_matches_jvm() {
    let mut c = Check::new("compile");
    for line in lines("compile.txt") {
        let f: Vec<&str> = line.split('\t').collect();
        let flags: i32 = f[0].parse().unwrap();
        let pat = unesc(f[1]);
        let want = if f[2] == "OK" {
            "OK".to_string()
        } else {
            format!("P\t{}\t{}\t{}", f[3], unesc(f[4]), unesc(f[5]))
        };
        let got = enc_compile(&Pattern::compile_flags(&pat, flags), &pat);
        c.eq(&format!("compile({pat:?}, {flags})"), &got, &want);
    }
    c.done();
}

#[test]
fn pattern_syntax_exception_accessors() {
    // Every invalid case in the fixture must round-trip description/index/pattern into
    // Java's exact multi-line getMessage().
    let mut c = Check::new("pse");
    for line in lines("compile.txt") {
        let f: Vec<&str> = line.split('\t').collect();
        if f[2] != "P" {
            continue;
        }
        let flags: i32 = f[0].parse().unwrap();
        let pat = unesc(f[1]);
        match Pattern::compile_flags(&pat, flags) {
            Ok(_) => c.eq(&format!("pse({pat:?})"), "OK", "err"),
            Err(e) => {
                c.eq(
                    &format!("pse({pat:?})"),
                    &format!("{}|{}|{}", e.get_index(), e.get_description(), e.get_pattern()),
                    &format!("{}|{}|{}", f[3], unesc(f[4]), pat),
                );
            }
        }
    }
    c.done();
}

#[test]
fn ops_match_jvm() {
    let mut c = Check::new("ops");
    for line in lines("ops.txt") {
        let f: Vec<&str> = line.split('\t').collect();
        let flags: i32 = f[0].parse().unwrap();
        let (pat, input, repl) = (unesc(f[1]), unesc(f[2]), unesc(f[3]));
        let p = match Pattern::compile_flags(&pat, flags) {
            Ok(p) => p,
            Err(e) => {
                c.eq(&format!("compile({pat:?}, {flags})"), &e.get_description().to_string(), "OK");
                continue;
            }
        };
        let tag = format!("({pat:?}, {flags}, {input:?}, {repl:?})");
        c.eq(&format!("matches{tag}"), &bool_str(p.matches(&input)), f[4]);
        c.eq(&format!("find{tag}"), &bool_str(p.find(&input)), f[5]);
        c.eq(&format!("split0{tag}"), &enc_arr(&p.split(&input, 0)), &want_arr(f[6]));
        c.eq(&format!("split2{tag}"), &enc_arr(&p.split(&input, 2)), &want_arr(f[7]));
        c.eq(&format!("splitm1{tag}"), &enc_arr(&p.split(&input, -1)), &want_arr(f[8]));
        c.eq(
            &format!("replaceAll{tag}"),
            &enc_str(p.replace_all(&input, &repl)),
            &want_field(f[9]),
        );
        c.eq(
            &format!("replaceFirst{tag}"),
            &enc_str(p.replace_first(&input, &repl)),
            &want_field(f[10]),
        );
    }
    c.done();
}

#[test]
fn string_ops_match_jvm() {
    let mut c = Check::new("strops");
    for line in lines("strops.txt") {
        let f: Vec<&str> = line.split('\t').collect();
        let (input, re, repl) = (unesc(f[0]), unesc(f[1]), unesc(f[2]));
        let tag = format!("({input:?}, {re:?}, {repl:?})");
        c.eq(&format!("matches{tag}"), &enc_bool_res(string_matches(&input, &re)), &want_field(f[3]));
        c.eq(
            &format!("split0{tag}"),
            &enc_arr_res(string_split(&input, &re, 0)),
            &want_arr(f[4]),
        );
        c.eq(
            &format!("split2{tag}"),
            &enc_arr_res(string_split(&input, &re, 2)),
            &want_arr(f[5]),
        );
        c.eq(
            &format!("splitm1{tag}"),
            &enc_arr_res(string_split(&input, &re, -1)),
            &want_arr(f[6]),
        );
        c.eq(
            &format!("replaceAll{tag}"),
            &enc_str(string_replace_all(&input, &re, &repl)),
            &want_field(f[7]),
        );
        c.eq(
            &format!("replaceFirst{tag}"),
            &enc_str(string_replace_first(&input, &re, &repl)),
            &want_field(f[8]),
        );
    }
    c.done();
}

#[test]
fn quote_matches_jvm() {
    let mut c = Check::new("quote");
    for line in lines("quote.txt") {
        let f: Vec<&str> = line.split('\t').collect();
        let s = unesc(f[0]);
        c.eq(&format!("quote({s:?})"), &quote(&s), &unesc(f[1]));
        c.eq(&format!("quoteReplacement({s:?})"), &quote_replacement(&s), &unesc(f[2]));
        // Pattern.quote(s) must match s literally, and only s.
        let p = Pattern::compile(&quote(&s)).expect("quote() must compile");
        c.eq(&format!("quoteRoundTrip({s:?})"), &bool_str(p.matches(&s)), "T");
    }
    c.done();
}

/// `\p{...}` translation: compares the matched code-point set against the JVM's.
#[test]
fn properties_match_jvm() {
    let full = std::env::var("JAVA_REGEX_FULL").is_ok();
    let mut c = Check::new("props");
    for line in lines("props.txt") {
        let f: Vec<&str> = line.split('\t').collect();
        let name = unesc(f[0]);
        let flags = if f[1] == "1" { CASE_INSENSITIVE } else { 0 };
        let src = format!("\\p{{{name}}}");
        let p = match Pattern::compile_flags(&src, flags) {
            Ok(p) => p,
            Err(e) => {
                c.eq(
                    &format!("prop({name}, {flags})"),
                    &format!("EX:P:{}:{}", e.get_index(), e.get_description()),
                    &want_field(f[2]),
                );
                continue;
            }
        };
        if f[2].starts_with("EX:") {
            c.eq(&format!("prop({name}, {flags})"), "OK", &want_field(f[2]));
            continue;
        }
        let mut want = vec![false; 0x110000];
        if !f[2].is_empty() {
            for r in f[2].split(',') {
                let (lo, hi) = r.split_once('-').unwrap();
                let lo = u32::from_str_radix(lo, 16).unwrap();
                let hi = u32::from_str_radix(hi, 16).unwrap();
                for cp in lo..=hi {
                    want[cp as usize] = true;
                }
            }
        }
        // Boundaries always; the interior on a stride (or exhaustively under JAVA_REGEX_FULL).
        let mut probes: Vec<u32> = Vec::new();
        for cp in 0..0x110000u32 {
            let edge = cp == 0 || want[cp as usize] != want[cp as usize - 1];
            if full || edge || cp % 37 == 0 {
                probes.push(cp);
            }
        }
        let mut bad = 0usize;
        let mut first = String::new();
        for cp in probes {
            let ch = match char::from_u32(cp) {
                Some(ch) => ch,
                None => continue, // surrogates cannot appear in a Rust str
            };
            let got = p.matches(&ch.to_string());
            if got != want[cp as usize] {
                bad += 1;
                if first.is_empty() {
                    first = format!("U+{cp:04X} got {got} want {}", want[cp as usize]);
                }
            }
        }
        c.eq(
            &format!("prop({name}, {flags})"),
            &if bad == 0 { "ok".to_string() } else { format!("{bad} bad, first {first}") },
            "ok",
        );
    }
    c.done();
}

/// No pattern, however malformed, may panic; compilation either succeeds or reports a
/// PatternSyntaxException, and a compiled pattern never panics on any input.
#[test]
fn never_panics() {
    let mut seed = 0x243f_6a88_85a3_08d3u64;
    let mut rnd = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    let alphabet: Vec<char> =
        "ab01.*+?|^$()[]{}-&\\/<>=!#:'\"~% \n\t\u{ac00}\u{1f600}\u{0}pPQEkxucNdswWG"
            .chars()
            .collect();
    let inputs = ["", "a", "abc", "\u{ac00}\u{1f600}", "a\nb\r\nc", "aaaaaaaaaaaaaaaaaaaa!"];
    for i in 0..20_000u32 {
        let n = (rnd() % 12) as usize + 1;
        let pat: String = (0..n).map(|_| alphabet[(rnd() as usize) % alphabet.len()]).collect();
        let flags = (rnd() % 0x200) as i32 & (CASE_INSENSITIVE | MULTILINE | DOTALL | UNIX_LINES);
        if let Ok(p) = Pattern::compile_flags(&pat, flags) {
            let input = inputs[(i as usize) % inputs.len()];
            let _ = p.matches(input);
            let _ = p.find(input);
            let _ = p.split(input, 0);
            let _ = p.replace_all(input, "$0x");
            let _ = p.replace_first(input, "\\$");
        }
    }
}
