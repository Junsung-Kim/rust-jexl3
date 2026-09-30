//! Debugger differential test: re-renders the parsed AST back to source and compares it with the
//! real org.apache.commons.jexl3.internal.Debugger.
//!
//! Three fixture sets, all oracle-produced:
//!  * `cases.jsonl` / `expected.jsonl` — tools/fuzz_gen.py `--ops parsed`, i.e.
//!    `JexlScript.getParsedText()` (indentation 2) for every source that parses.
//!  * `rt_cases.jsonl` / `rt_expected.jsonl` — the same cases with the *rendered* text as source,
//!    which pins Java's (non-)idempotence of render -> parse -> render.
//!  * `api_cases.jsonl` / `api_expected.jsonl` — written by `tests/data/debugger/gen/DebugGen.java`
//!    (it lives in the library's own package so it can reach the package-private surface):
//!    `getParsedText(n)` for several indentations, and per AST node `debug(node)`'s start/end,
//!    `data(node)` and `depth(n).data(node)`.
mod common;

use common::json::{self, Json};
use rust_jexl3::internal::debugger::Debugger;
use rust_jexl3::internal::engine;
use rust_jexl3::java::string::JString;
use rust_jexl3::jexl_features::JexlFeatures;
use rust_jexl3::jexl_info::JexlInfo;
use rust_jexl3::parser::jexl_node::{NodeRef, Parsed};
use rust_jexl3::parser::parser::Parser;

// --------------------------------------------------------------------------------- engine config
// (same shape as tests/parser_oracle.rs: only what the parser reads matters here)

fn features_of(spec: Option<&Json>) -> JexlFeatures {
    let mut f = JexlFeatures::new();
    if let Some(Json::Obj(kv)) = spec {
        for (k, v) in kv {
            let b = v.as_bool().unwrap_or(false);
            f = match k.as_str() {
                "register" => f.register(b),
                "localVar" => f.local_var(b),
                "sideEffect" => f.side_effect(b),
                "sideEffectGlobal" => f.side_effect_global(b),
                "arrayReferenceExpr" => f.array_reference_expr(b),
                "newInstance" => f.new_instance(b),
                "loops" => f.loops(b),
                "lambda" => f.lambda(b),
                "methodCall" => f.method_call(b),
                "structuredLiteral" => f.structured_literal(b),
                "pragma" => f.pragma(b),
                "annotation" => f.annotation(b),
                "script" => f.script(b),
                "lexical" => f.lexical(b),
                "lexicalShade" => f.lexical_shade(b),
                "reservedNames" => {
                    let names: Vec<String> = v.arr().unwrap_or(&[]).iter().filter_map(Json::string).collect();
                    f.reserved_names(names)
                }
                other => panic!("unknown feature {}", other),
            };
        }
    }
    f
}

fn parse_features(engine: Option<&Json>) -> JexlFeatures {
    let mut f = features_of(engine.and_then(|e| e.get("features")));
    let mut ns: Vec<String> = Vec::new();
    if let Some(Json::Obj(kv)) = engine.and_then(|e| e.get("namespaces")) {
        ns.extend(kv.iter().map(|(k, _)| k.clone()));
    }
    if !ns.is_empty() {
        f = f.namespace_test(Some(std::sync::Arc::new(move |n: &str| ns.iter().any(|x| x == n))));
    }
    let script = engine.and_then(|e| e.get("features")).and_then(|f| f.get("script")).and_then(Json::as_bool);
    f.script(script.unwrap_or(true))
}

/// Parses a fixture case the way Engine.createScript / createExpression does.
fn parse_case(parser: &mut Parser, case: &Json) -> Option<Parsed> {
    let src = case.get("src").and_then(Json::string).expect("src");
    let kind = case.get("kind").and_then(Json::string).unwrap_or_else(|| "script".into());
    let params: Option<Vec<String>> =
        case.get("params").and_then(Json::arr).map(|a| a.iter().filter_map(Json::string).collect());
    let mut features = parse_features(case.get("engine"));
    if kind == "expression" {
        features = features.script(false);
    }
    let src = engine::trim_source(&src);
    let info = JexlInfo::new(Some("case".to_string()), 1, 1);
    parser.parse(Some(info), &features, &src, params.as_deref()).ok()
}

fn read(path: &str, var: &str) -> Vec<Json> {
    let p = std::env::var(var).unwrap_or_else(|_| path.into());
    std::fs::read_to_string(&p)
        .unwrap_or_else(|e| panic!("{}: {}", p, e))
        .lines()
        .map(|l| json::parse(l).expect("json"))
        .collect()
}

fn want_str(v: &Json) -> JString {
    JString::from_units(v.str16().expect("string"))
}

fn report(failures: &[String], n: usize, what: &str) {
    assert!(n > 0, "no cases");
    assert!(
        failures.is_empty(),
        "{} of {} {} differ:\n{}",
        failures.len(),
        n,
        what,
        failures.iter().take(10).cloned().collect::<Vec<_>>().join("\n")
    );
}

// ------------------------------------------------------------------------------ getParsedText(2)

/// Runs one `parsed` fixture set: every case must parse and render exactly like the oracle's
/// `JexlScript.getParsedText()`.
fn parsed_set(cases: &[Json], expected: &[Json], what: &str) -> usize {
    let mut parsers: std::collections::HashMap<String, Parser> = std::collections::HashMap::new();
    let mut failures: Vec<String> = Vec::new();
    let mut n = 0usize;
    for (case, want) in cases.iter().zip(expected.iter()) {
        n += 1;
        let key = case.get("engine").map(json::to_string).unwrap_or_default();
        let parser = parsers.entry(key).or_default();
        let parsed = parse_case(parser, case);
        let id = case.get("id").and_then(Json::string).unwrap_or_default();
        let src = case.get("src").and_then(Json::string).unwrap_or_default();
        match (want.get("parsed"), parsed) {
            (None, None) => {}
            (None, Some(p)) => failures.push(format!(
                "{}: src={:?}\n  oracle failed to parse, we rendered {:?}",
                id,
                src,
                Debugger::new().data_indent(p.node(), 2).to_rust()
            )),
            (Some(_), None) => failures.push(format!("{}: src={:?}\n  we failed to parse", id, src)),
            (Some(w), Some(p)) => {
                let got = Debugger::new().data_indent(p.node(), 2);
                if got != want_str(w) {
                    failures.push(format!(
                        "{}: src={:?}\n  want {:?}\n  got  {:?}",
                        id,
                        src,
                        want_str(w).to_rust(),
                        got.to_rust()
                    ));
                }
            }
        }
    }
    report(&failures, n, what);
    n
}

#[test]
fn parsed_text_matches_oracle() {
    let cases = read("tests/data/debugger/cases.jsonl", "DEBUGGER_CASES");
    let expected = read("tests/data/debugger/expected.jsonl", "DEBUGGER_EXPECTED");
    assert_eq!(cases.len(), expected.len());
    let n = parsed_set(&cases, &expected, "parsed-text cases");
    assert!(n >= 8000, "only {} cases", n);
}

/// True when the UTF-16 text has an unpaired surrogate, i.e. no Rust `&str` can hold it.
fn has_lone_surrogate(u: &[u16]) -> bool {
    let mut i = 0;
    while i < u.len() {
        let c = u[i];
        if (0xD800..0xDC00).contains(&c) {
            if i + 1 >= u.len() || !(0xDC00..0xE000).contains(&u[i + 1]) {
                return true;
            }
            i += 1;
        } else if (0xDC00..0xE000).contains(&c) {
            return true;
        }
        i += 1;
    }
    false
}

#[test]
fn round_trip_matches_oracle() {
    let cases = read("tests/data/debugger/rt_cases.jsonl", "DEBUGGER_RT_CASES");
    let expected = read("tests/data/debugger/rt_expected.jsonl", "DEBUGGER_RT_EXPECTED");
    assert_eq!(cases.len(), expected.len());
    // A rendered script whose string literal holds an unpaired surrogate cannot be fed back in:
    // Parser::parse takes a Rust &str, while Java parses a UTF-16 String. Those cases are counted
    // (and the count pinned) rather than compared.
    let mut skipped = 0;
    let (kept_c, kept_e): (Vec<Json>, Vec<Json>) = cases
        .into_iter()
        .zip(expected)
        .filter(|(c, _)| {
            let keep = !has_lone_surrogate(c.get("src").and_then(Json::str16).expect("src"));
            if !keep {
                skipped += 1;
            }
            keep
        })
        .unzip();
    if std::env::var("DEBUGGER_RT_CASES").is_err() {
        assert_eq!(skipped, 141, "unrepresentable-source count drifted");
    }
    parsed_set(&kept_c, &kept_e, "round-trip cases");
}

/// Java's render -> parse -> render is *not* idempotent. Pinned from the committed fixtures so the
/// divergence is a recorded fact, not a surprise: `a..b` re-renders as `a .. b`, `{\n}var l;` as
/// `{  };\nvar l;`, `x.1e3` re-parses as a new statement, and 793 renderings do not parse at all
/// (string literals are re-emitted raw, so a literal newline or a lone surrogate breaks the lexer).
#[test]
fn round_trip_divergences_are_javas_own() {
    // the committed fixtures, never an overridden campaign set
    let first = read("tests/data/debugger/expected.jsonl", "__none");
    let again = read("tests/data/debugger/rt_expected.jsonl", "__none");
    let mut reparse_fail = 0;
    let mut differs = 0;
    for (a, b) in first.iter().zip(again.iter()) {
        match b.get("parsed") {
            None => reparse_fail += 1,
            Some(p) => {
                if want_str(p) != want_str(a.get("parsed").expect("parsed")) {
                    differs += 1;
                }
            }
        }
    }
    assert_eq!((reparse_fail, differs), (793, 16), "round-trip fixture drifted");
}

// ---------------------------------------------------------------------------------- the full API

/// The code units Debugger.QUOTED_IDENTIFIER (`[\s]|[\p{Punct}&&[^@#\$_]]`) matches, measured on
/// the JVM by DebugGen and re-checked against our hand-rolled predicate.
fn quoted_pin(rec: &Json) -> Vec<u16> {
    rec.get("chars").and_then(Json::arr).expect("chars").iter().map(|c| c.as_i64().expect("num") as u16).collect()
}

fn nth_node(root: NodeRef<'_>, want: usize) -> Option<NodeRef<'_>> {
    fn walk<'a>(n: NodeRef<'a>, i: &mut usize, want: usize) -> Option<NodeRef<'a>> {
        if *i == want {
            return Some(n);
        }
        *i += 1;
        for c in n.children() {
            if let Some(f) = walk(c, i, want) {
                return Some(f);
            }
        }
        None
    }
    walk(root, &mut 0, want)
}

#[test]
fn debugger_api_matches_oracle() {
    let cases = read("tests/data/debugger/api_cases.jsonl", "DEBUGGER_API_CASES");
    let expected = read("tests/data/debugger/api_expected.jsonl", "DEBUGGER_API_EXPECTED");
    assert_eq!(cases.len(), expected.len());
    let mut parser = Parser::new();
    let mut failures: Vec<String> = Vec::new();
    let mut n = 0usize;
    let mut nodes_checked = 0usize;
    for (case, want) in cases.iter().zip(expected.iter()) {
        let id = case.get("id").and_then(Json::string).unwrap_or_default();
        if id == "__quoted" {
            // the pinned character set, checked against our predicate below
            for c in 0..=u16::MAX {
                let got = Debugger::need_quotes(&JString::from_units(&[c]));
                let wanted = quoted_pin(want).contains(&c);
                assert_eq!(got, wanted, "need_quotes(U+{:04X})", c);
            }
            continue;
        }
        n += 1;
        let src = case.get("src").and_then(Json::string).expect("src");
        let parsed = parse_case(&mut parser, case).unwrap_or_else(|| panic!("{}: {:?} must parse", id, src));
        let root = parsed.node();
        let p2 = want_str(want.get("p2").expect("p2"));
        for indent in [0, 1, 2, 4] {
            let w = want_str(want.get(&format!("p{}", indent)).expect("p"));
            let got = Debugger::new().data_indent(root, indent);
            if got != w {
                failures.push(format!(
                    "{}: src={:?} indent={}\n  want {:?}\n  got  {:?}",
                    id,
                    src,
                    indent,
                    w.to_rust(),
                    got.to_rust()
                ));
            }
        }
        // DebugGen measured that debug(node) always renders the whole script, i.e. getParsedText(2)
        assert!(want.get("teq").and_then(Json::as_bool).expect("teq"));
        for (i, wnode) in want.get("nodes").and_then(Json::arr).expect("nodes").iter().enumerate() {
            let node = nth_node(root, i).unwrap_or_else(|| panic!("{}: no node {}", id, i));
            nodes_checked += 1;
            let cls = wnode.get("c").and_then(Json::string).expect("c");
            if node.class_name() != cls {
                failures.push(format!("{}: node {} is {} want {}", id, i, node.class_name(), cls));
                continue;
            }
            let mut dbg = Debugger::new();
            let found = dbg.debug(node);
            let mut bad = |field: &str, w: String, g: String| {
                failures.push(format!("{}: src={:?} node {} ({}) {}\n  want {}\n  got  {}", id, src, i, cls, field, w, g))
            };
            if found != wnode.get("found").and_then(Json::as_bool).expect("found") {
                bad("found", format!("{:?}", !found), format!("{:?}", found));
            }
            if dbg.to_jstring() != p2 {
                bad("debug text", format!("{:?}", p2.to_rust()), format!("{:?}", dbg.to_jstring().to_rust()));
            }
            for (field, got) in [("s", dbg.start()), ("e", dbg.end())] {
                let w = wnode.get(field).and_then(Json::as_i64).expect("offset") as i32;
                if got != w {
                    bad(field, w.to_string(), got.to_string());
                }
            }
            let got = Debugger::new().data(node);
            let w = want_str(wnode.get("data").expect("data"));
            if got != w {
                bad("data", format!("{:?}", w.to_rust()), format!("{:?}", got.to_rust()));
            }
            for d in [1, 2] {
                let got = Debugger::new().depth(d).data(node);
                let w = want_str(wnode.get(&format!("d{}", d)).expect("d"));
                if got != w {
                    bad(&format!("depth({})", d), format!("{:?}", w.to_rust()), format!("{:?}", got.to_rust()));
                }
            }
        }
    }
    assert!(nodes_checked > 10000, "only {} nodes", nodes_checked);
    report(&failures, n, "debugger API cases");
}

// ------------------------------------------------------------------------------------ unit tests

fn render(src: &str) -> String {
    let mut parser = Parser::new();
    let f = JexlFeatures::new().script(true);
    let p = parser.parse(None, &f, &engine::trim_source(src), None).expect("parse");
    Debugger::new().data(p.node()).to_rust()
}

#[test]
fn reset_restores_the_defaults() {
    let mut parser = Parser::new();
    let f = JexlFeatures::new().script(true);
    let p = parser.parse(None, &f, "{ a; b }", None).expect("parse");
    let mut dbg = Debugger::default();
    dbg.indentation(0).depth(1);
    assert_eq!(dbg.data(p.node()).to_rust(), "...");
    dbg.reset();
    assert_eq!(dbg.start(), 0);
    assert_eq!(dbg.end(), 0);
    assert_eq!(dbg.to_jstring().to_rust(), "");
    // back to indent 2, unbounded depth
    assert_eq!(dbg.data(p.node()).to_rust(), "{\n  a;\n  b;\n}");
}

#[test]
fn indentation_is_clamped_to_zero() {
    let mut parser = Parser::new();
    let f = JexlFeatures::new().script(true);
    // `{ a }` is a set literal; `{ a; { b } }` is a block
    let p = parser.parse(None, &f, "{ a; { b } }", None).expect("parse");
    assert_eq!(Debugger::new().data_indent(p.node(), -3).to_rust(), "{ a; { b }; }");
    assert_eq!(Debugger::new().data_indent(p.node(), 0).to_rust(), "{ a; { b }; }");
    assert_eq!(Debugger::new().data_indent(p.node(), 1).to_rust(), "{\n a;\n { b };\n}");
    assert_eq!(Debugger::new().data_indent(p.node(), 4).to_rust(), "{\n    a;\n    { b };\n}");
}

#[test]
fn detail_of_locates_the_cause() {
    let mut parser = Parser::new();
    let f = JexlFeatures::new().script(true);
    let p = parser.parse(None, &f, "a + b * c", None).expect("parse");
    let root = p.node();
    // the whole script: start 0, end = length
    let d = Debugger::detail_of(&p.handle()).expect("detail");
    assert_eq!((d.start, d.end), (0, 9));
    assert_eq!(d.text.to_rust(), "a + b * c");
    // a sub-node: the MulNode `b * c`
    let mul = nth_node(root, 3).expect("node");
    assert_eq!(mul.class_name(), "ASTMulNode");
    let h = rust_jexl3::parser::jexl_node::NodeHandle::new(p.ast.clone(), mul.id);
    let d = Debugger::detail_of(&h).expect("detail");
    assert_eq!((d.start, d.end), (4, 9));
}

/// An empty script renders to nothing, so `debug` cannot locate the cause and
/// JexlException.detailedInfo attaches no source snippet.
#[test]
fn empty_script_has_no_detail() {
    let mut parser = Parser::new();
    let f = JexlFeatures::new().script(true);
    let p = parser.parse(None, &f, "", None).expect("parse");
    assert_eq!(p.node().num_children(), 0);
    let mut dbg = Debugger::new();
    assert!(!dbg.debug(p.node()));
    assert_eq!(dbg.to_jstring().to_rust(), "");
    assert_eq!(Debugger::detail_of(&p.handle()), None);
}

/// Deeply nested sources: the renderer recurses once per AST level, exactly like Java's
/// `accept` -> `jjtAccept` -> `visit` chain, and overflows the stack the same way Java raises
/// StackOverflowError. It is never the binding constraint: the recursive-descent parser needs far
/// more stack per level, so any tree the parser could build renders. Run on a 64MB stack, the same
/// spirit as the oracle's `-Xss16m`.
#[test]
fn deep_nesting_does_not_crash() {
    std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(|| {
            let mut parser = Parser::new();
            let f = JexlFeatures::new().script(true);
            let depth = 100;
            let src = format!("{}a{}", "(".repeat(depth), ")".repeat(depth));
            let p = parser.parse(None, &f, &src, None).expect("parse");
            let out = Debugger::new().data(p.node());
            assert_eq!(out.len(), 2 * depth + 1);
            // and the cause offsets still land on the innermost node
            let mut inner = p.node();
            while inner.num_children() > 0 {
                inner = inner.child(0);
            }
            assert_eq!(inner.class_name(), "ASTIdentifier");
            let mut dbg = Debugger::new();
            assert!(dbg.debug(inner));
            assert_eq!((dbg.start(), dbg.end()), (depth as i32, depth as i32 + 1));
        })
        .expect("spawn")
        .join()
        .expect("join");
}

/// Lone surrogates survive the render (the text is UTF-16, not a Rust String): the source escape
/// `\ud83d` becomes one unpaired code unit in the literal, which the renderer emits as-is.
#[test]
fn lone_surrogates_are_preserved() {
    let mut parser = Parser::new();
    let f = JexlFeatures::new().script(true);
    let p = parser.parse(None, &f, "'\\ud83d'", None).expect("parse");
    let out = Debugger::new().data(p.node());
    assert_eq!(out.units(), &[b'\'' as u16, 0xD83D, b'\'' as u16]);
}

/// A readable sample of the shapes the visitor special-cases; every expectation is the oracle's
/// own `getParsedText(2)` (these sources are in DebugGen's EXTRA list).
#[test]
fn renders_the_shapes_the_visitor_special_cases() {
    // parenthesised by precedence
    assert_eq!(render("(a + b) * c"), "(a + b) * c");
    assert_eq!(render("a * (b + c)"), "a * (b + c)");
    assert_eq!(render("a | b & c"), "a | b & c");
    assert_eq!(render("a && (b || c)"), "a && (b || c)");
    assert_eq!(render("~(a + b)"), "~(a + b)");
    assert_eq!(render("-(a + b)"), "-(a + b)");
    assert_eq!(render("+a"), "+a");
    // identifiers and property names
    assert_eq!(render("x.'a b'"), "x.'a b'");
    assert_eq!(render("x.size"), "x.'size'");
    assert_eq!(render("x.`${y}`"), "x.`${y}`");
    assert_eq!(render("a\\ b"), "a\\ b");
    assert_eq!(render("ns:fn(1, 2)"), "ns:fn(1, 2)");
    assert_eq!(render("'it\\'s'"), "'it\\'s'");
    // statements
    assert_eq!(render("while(x);"), "while (x) ;");
    assert_eq!(render("for(var x : y);"), "for(var x : y) ;");
    assert_eq!(render("do ; while(x)"), "do ; while (x)");
    assert_eq!(render("if (a) b; else if (c) d; else e"), "if (a) b;\n else if (c) d;\n else e;\n");
    assert_eq!(render("@ann(1) x = 2"), "@ann(1) x = 2;\n");
    assert_eq!(render("var f = function(a, b) { a + b }"), "var f = function(a, b) {\n  a + b;\n}");
    assert_eq!(render("(a, b)->{ a + b }"), "(a, b)->{\n  a + b;\n}");
    // literals
    assert_eq!(render("{:}"), "{ : }");
    assert_eq!(render("{ 1, 2 }"), "{ 1,2 }");
    assert_eq!(render("[ 1, 2, ... ]"), "[ 1, 2, ... ]");
}
