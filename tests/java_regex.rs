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

/// Descriptions of constructs this port cannot express on fancy-regex; each is pinned by a
/// `pinned_*` test below.
fn unsupported(desc: &str) -> bool {
    desc.starts_with("Unsupported")
}

/// The one JDK diagnostic this port cannot reproduce: it needs the JDK's `TreeInfo` study
/// pass over a node tree this port never builds.
const LOOKBEHIND_MAX: &str = "Look-behind group does not have an obvious maximum length";

/// Java's `Matcher.find()` steps one UTF-16 code unit past an empty match, so an empty match
/// can land inside a surrogate pair and split it; the halves are then unpaired surrogates.
/// A Rust `String` cannot hold those at all, so such expectations are skipped.
const LONE_SURROGATE: &str = "expected value contains an unpaired surrogate";

/// Java indexes by UTF-16 code unit, so a zero-width match can land *inside* a surrogate
/// pair.  That position does not exist under Rust's code-point indexing.
const MID_SURROGATE: &str = "zero-width match inside a surrogate pair (UTF-16 indexing)";

fn has_supplementary(s: &str) -> bool {
    s.chars().any(|c| c as u32 > 0xFFFF)
}

/// True when the pattern can match with zero width, which is when UTF-16 indexing can put
/// the JVM at a position this port has no equivalent for.
fn zero_width_capable(p: &Pattern) -> bool {
    ["", "a", "ab", "a b", "ab1_", "\n", "a\r\nb"]
        .iter()
        .any(|s| p.find_ranges(s).iter().any(|&(a, b)| a == b))
}

fn has_lone_surrogate(raw: &str) -> bool {
    let b = raw.as_bytes();
    let mut units: Vec<u16> = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\\' && b.get(i + 1) == Some(&b'\\') {
            units.push(b'\\' as u16);
            i += 2;
        } else if b[i] == b'\\' && b.get(i + 1) == Some(&b'u') {
            units.push(u16::from_str_radix(&raw[i + 2..i + 6], 16).unwrap());
            i += 6;
        } else {
            units.push(b[i] as u16);
            i += 1;
        }
    }
    let mut i = 0;
    while i < units.len() {
        let u = units[i];
        if (0xD800..0xDC00).contains(&u) {
            match units.get(i + 1) {
                Some(l) if (0xDC00..0xE000).contains(l) => i += 2,
                _ => return true,
            }
        } else if (0xDC00..0xE000).contains(&u) {
            return true;
        } else {
            i += 1;
        }
    }
    false
}

struct Check {
    name: &'static str,
    count: usize,
    skipped: std::collections::BTreeMap<String, usize>,
    failures: Vec<String>,
}

impl Check {
    fn new(name: &'static str) -> Self {
        Check { name, count: 0, skipped: std::collections::BTreeMap::new(), failures: Vec::new() }
    }
    /// Records a case skipped because of a pinned divergence.
    fn skip(&mut self, reason: &str) {
        *self.skipped.entry(reason.to_string()).or_default() += 1;
    }
    /// Compares against a fixture field, skipping expectations Rust cannot represent.
    fn eq_field(&mut self, what: &str, got: &str, raw: &str, want: String) {
        if has_lone_surrogate(raw) {
            self.skip(LONE_SURROGATE);
        } else if raw.contains(LOOKBEHIND_MAX) {
            self.skip(LOOKBEHIND_MAX);
        } else {
            self.eq(what, got, &want);
        }
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
        let skipped: usize = self.skipped.values().sum();
        for (reason, n) in &self.skipped {
            eprintln!("{}: skipped {n} (pinned divergence: {reason})", self.name);
        }
        assert!(
            skipped * 50 <= self.count + skipped,
            "{}: {skipped} skipped of {} - pinned divergences must stay marginal",
            self.name,
            self.count + skipped
        );
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
        let r = Pattern::compile_flags(&pat, flags);
        if let Err(e) = &r {
            if unsupported(e.get_description()) {
                c.skip(e.get_description());
                continue;
            }
        }
        if f[2] == "P" && unesc(f[4]) == LOOKBEHIND_MAX {
            c.skip(LOOKBEHIND_MAX);
            continue;
        }
        let got = enc_compile(&r, &pat);
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
        if unesc(f[4]) == LOOKBEHIND_MAX {
            c.skip(LOOKBEHIND_MAX);
            continue;
        }
        match Pattern::compile_flags(&pat, flags) {
            Ok(_) => c.eq(&format!("pse({pat:?})"), "OK", "err"),
            Err(e) if unsupported(e.get_description()) => c.skip(e.get_description()),
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
            Err(e) if unsupported(e.get_description()) => {
                c.skip(e.get_description());
                continue;
            }
            Err(e) => {
                c.eq(&format!("compile({pat:?}, {flags})"), e.get_description(), "OK");
                continue;
            }
        };
        let tag = format!("({pat:?}, {flags}, {input:?}, {repl:?})");
        c.eq(&format!("matches{tag}"), &bool_str(p.matches(&input)), f[4]);
        if has_supplementary(&input) && zero_width_capable(&p) {
            c.skip(MID_SURROGATE);
            continue;
        }
        c.eq(&format!("find{tag}"), &bool_str(p.find(&input)), f[5]);
        c.eq_field(&format!("split0{tag}"), &enc_arr(&p.split(&input, 0)), f[6], want_arr(f[6]));
        c.eq_field(&format!("split2{tag}"), &enc_arr(&p.split(&input, 2)), f[7], want_arr(f[7]));
        c.eq_field(&format!("splitm1{tag}"), &enc_arr(&p.split(&input, -1)), f[8], want_arr(f[8]));
        c.eq_field(
            &format!("replaceAll{tag}"),
            &enc_str(p.replace_all(&input, &repl)),
            f[9],
            want_field(f[9]),
        );
        c.eq_field(
            &format!("replaceFirst{tag}"),
            &enc_str(p.replace_first(&input, &repl)),
            f[10],
            want_field(f[10]),
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
        if let Err(e) = Pattern::compile(&re) {
            if unsupported(e.get_description()) {
                c.skip(e.get_description());
                continue;
            }
        }
        let tag = format!("({input:?}, {re:?}, {repl:?})");
        if has_supplementary(&input)
            && Pattern::compile(&re).is_ok_and(|p| zero_width_capable(&p))
        {
            c.skip(MID_SURROGATE);
            continue;
        }
        c.eq_field(
            &format!("matches{tag}"),
            &enc_bool_res(string_matches(&input, &re)),
            f[3],
            want_field(f[3]),
        );
        c.eq_field(
            &format!("split0{tag}"),
            &enc_arr_res(string_split(&input, &re, 0)),
            f[4],
            want_arr(f[4]),
        );
        c.eq_field(
            &format!("split2{tag}"),
            &enc_arr_res(string_split(&input, &re, 2)),
            f[5],
            want_arr(f[5]),
        );
        c.eq_field(
            &format!("splitm1{tag}"),
            &enc_arr_res(string_split(&input, &re, -1)),
            f[6],
            want_arr(f[6]),
        );
        c.eq_field(
            &format!("replaceAll{tag}"),
            &enc_str(string_replace_all(&input, &re, &repl)),
            f[7],
            want_field(f[7]),
        );
        c.eq_field(
            &format!("replaceFirst{tag}"),
            &enc_str(string_replace_first(&input, &re, &repl)),
            f[8],
            want_field(f[8]),
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
            Err(e) if unsupported(e.get_description()) => {
                c.skip(e.get_description());
                continue;
            }
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
    for i in 0..40_000u32 {
        let n = (rnd() % 12) as usize + 1;
        // Half the campaign is drawn from regex metacharacters, half is arbitrary text
        // (every Rust `&str` a caller could hand us, including unassigned code points).
        let pat: String = (0..n)
            .map(|_| {
                if i % 2 == 0 {
                    alphabet[(rnd() as usize) % alphabet.len()]
                } else {
                    loop {
                        if let Some(c) = char::from_u32((rnd() % 0x11_0000) as u32) {
                            return c;
                        }
                    }
                }
            })
            .collect();
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

// --------------------------------------------------------------- pinned divergences
//
// Each case below is a construct where this port cannot reproduce the JVM.  The assertion
// records what it does instead, so a future change to the behaviour is a test failure
// rather than a silent drift.  The JVM behaviour quoted in each comment was measured with
// Corretto 25.

fn desc(regex: &str) -> String {
    Pattern::compile(regex).err().map_or_else(|| "OK".to_string(), |e| e.get_description().into())
}

#[test]
fn pinned_unsupported_constructs() {
    // JVM: compiles; matches one extended grapheme cluster.
    assert_eq!(desc(r"\X"), "Unsupported: grapheme cluster matching (\\X, \\b{g})");
    assert_eq!(desc(r"\b{g}"), "Unsupported: grapheme cluster matching (\\X, \\b{g})");
    // JVM: compiles to the code point named by the Unicode character database.
    assert_eq!(
        desc(r"\N{LATIN SMALL LETTER A}"),
        "Unsupported: \\N{name} needs the Unicode name table"
    );
    // JVM: "Unknown character name [NOSUCHNAME]" - this port cannot tell known from unknown.
    assert_eq!(desc(r"\N{NOSUCHNAME}"), "Unsupported: \\N{name} needs the Unicode name table");
    // JVM: matches every character Character.isMirrored() accepts.
    assert_eq!(desc(r"\p{javaMirrored}"), "Unsupported character property: javaMirrored");
    // JVM: CANON_EQ matches canonically equivalent sequences.
    let e = Pattern::compile_flags("a", CANON_EQ).unwrap_err();
    assert_eq!(e.get_description(), "Unsupported: CANON_EQ needs Unicode normalisation");
    assert_eq!(e.get_index(), -1);
}

#[test]
fn pinned_variable_length_lookbehind() {
    // A top-level alternation of constant-width branches is distributed and works.
    let p = Pattern::compile("(?<=ab|cde)x").unwrap();
    assert!(p.find("abx"));
    assert!(p.find("cdex"));
    assert!(!p.find("zzx"));
    let n = Pattern::compile("(?<!ab|cde)x").unwrap();
    assert!(!n.find("abx"));
    assert!(n.find("zzx"));
    // Anything else fancy-regex cannot express: the JVM compiles `(?<=a*)` and matches.
    assert!(desc("(?<=a*)").starts_with("Unsupported by fancy-regex"));
    // And where the JVM itself refuses, it says "Look-behind group does not have an obvious
    // maximum length" at the index of the group body; this port reports the fancy-regex
    // rejection (or a later parse error) instead.
    assert_ne!(desc(r"(?<!&\pL%\2."), LOOKBEHIND_MAX);
}

#[test]
fn pinned_utf16_indexing() {
    // The JVM's Matcher steps one UTF-16 code unit past an empty match, so `"a\u{1F600}b"`
    // splits on `x?` into four pieces, two of them unpaired surrogates.  Rust strings
    // cannot hold those, so this port steps a whole code point.
    let p = Pattern::compile("x?").unwrap();
    assert_eq!(p.split("a\u{1f600}b", -1), ["a", "\u{1f600}", "b", ""]);
    // Likewise `\B` holds between the two halves of a surrogate pair on the JVM (find() is
    // true there); no such position exists here.
    assert!(!Pattern::compile(r"\B").unwrap().find("a\u{1f600}b"));
}

#[test]
fn pinned_backtracking_and_word_marks() {
    // The JVM would backtrack (potentially for ever); this port gives up at the limit and
    // reports "no match".
    let p = Pattern::compile("(x+x+)+y").unwrap();
    assert!(!p.find(&"x".repeat(60)));
    assert!(p.find(&("x".repeat(8) + "y")));
    // The JVM counts a non-spacing mark as a word character when it follows a base letter,
    // scanning back over any number of marks.  That needs a variable-length look-behind, so
    // this port bounds the scan at MARK_RUN = 8 marks.  Measured on Corretto 25:
    //   "a" + 8 marks  -> \b at {0, 9};   "a" + 9 marks -> \b at {0, 10}
    let b = Pattern::compile(r"\b").unwrap();
    let eight = format!("a{}", "\u{301}".repeat(8));
    assert_eq!(b.find_ranges(&eight), [(0, 0), (eight.len(), eight.len())]);
    // One mark past the bound the JVM still reports a boundary at the end; this port does
    // not, because the look-behind alternation stops at 8.
    let nine = format!("a{}", "\u{301}".repeat(9));
    assert_eq!(b.find_ranges(&nine), [(0, 0)]);
}

#[test]
fn error_types_display_as_java_messages() {
    let e = Pattern::compile("a(").unwrap_err();
    assert_eq!(e.to_string(), e.get_message());
    // The caret line is omitted when the index is not inside the pattern.
    assert_eq!(e.to_string(), "Unclosed group near index 2\na(");
    let e2 = Pattern::compile("a(b").unwrap_err();
    assert_eq!(e2.get_message(), "Unclosed group near index 3\na(b");
    let e3 = Pattern::compile("[a-").unwrap_err();
    assert_eq!(e3.get_message(), "Illegal character range near index 3\n[a-");
    assert_eq!(JavaRegexError::from(e.clone()).to_string(), e.get_message());
    assert_eq!(JavaRegexError::IllegalArgument("x".into()).to_string(), "x");
    assert_eq!(JavaRegexError::IndexOutOfBounds("No group 3".into()).to_string(), "No group 3");
    // getMessage() drops the caret line when the index is past the end of the pattern.
    let e = Pattern::compile("\\").unwrap_err();
    assert_eq!(e.get_message(), "Unescaped trailing backslash near index 1\n\\");
    assert_eq!(e.get_pattern(), "\\");
}

#[test]
fn pinned_nesting_depth_limit() {
    // The JVM compiles this (it recurses until the JVM stack runs out); this port refuses
    // at a fixed depth so that no input can overflow the Rust stack.
    let deep = format!("{}a{}", "(".repeat(400), ")".repeat(400));
    assert_eq!(desc(&deep), "Unsupported: pattern nests too deeply");
    assert_eq!(desc(&format!("{}a", "[".repeat(400))), "Unsupported: pattern nests too deeply");
    // Shallow nesting still compiles and matches (fancy-regex has its own, lower limit).
    let ok = format!("{}a{}", "(".repeat(30), ")".repeat(30));
    assert!(Pattern::compile(&ok).unwrap().matches("a"));
}

#[test]
fn find_ranges_follows_java_find() {
    let p = Pattern::compile("a*").unwrap();
    assert_eq!(p.find_ranges("aab"), [(0, 2), (2, 2), (3, 3)]);
    assert_eq!(Pattern::compile("").unwrap().find_ranges("ab"), [(0, 0), (1, 1), (2, 2)]);
}

#[test]
fn compile_flags_checked_rejects_unknown_bits() {
    // `new Pattern(String,int)` throws IllegalArgumentException("Unknown flag 0x...").
    match Pattern::compile_flags_checked("a", 0x400) {
        Err(JavaRegexError::IllegalArgument(m)) => assert_eq!(m, "Unknown flag 0x400"),
        other => panic!("{other:?}"),
    }
    assert!(Pattern::compile_flags_checked("a", CASE_INSENSITIVE).is_ok());
    // UNICODE_CHARACTER_CLASS implies UNICODE_CASE, as the JDK constructor does.
    let p = Pattern::compile_flags("a", UNICODE_CHARACTER_CLASS).unwrap();
    assert_eq!(p.flags(), UNICODE_CHARACTER_CLASS | UNICODE_CASE);
}

/// The two JEXL entry points: `=~` compiles a literal, or coerces both sides to strings.
#[test]
fn jexl_contains_paths() {
    let p = Pattern::compile("^\\d{3}-\\d{4}$").unwrap();
    assert!(p.matches("555-1234"));
    assert!(!p.matches("x555-1234"));
    assert_eq!(string_matches("hello", "h.*o"), Ok(true));
    assert_eq!(p.translated(), p.translated()); // translation is stable
}
