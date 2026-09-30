//! Parser differential test: parses the same sources as the real
//! org.apache.commons.jexl3.parser.Parser and compares the whole tree (class, line, column, node
//! attributes, children), the parse exception (class and exact message), `getVariables()`,
//! parameters, locals and pragmas. Fixtures come from tools/fuzz_gen.py + the oracle's `ast` mode.
#![allow(clippy::unwrap_or_default)]
mod common;

use common::encode::encode;
use common::json::{self, Json};
use rust_jexl3::internal::engine;
use rust_jexl3::jexl_features::JexlFeatures;
use rust_jexl3::jexl_info::JexlInfo;
use rust_jexl3::parser::jexl_node::{NodeRef, Parsed};
use rust_jexl3::parser::parser::Parser;
use rust_jexl3::value::Value;

fn strings(v: &[String]) -> Json {
    Json::Arr(v.iter().map(|s| Json::str(s)).collect())
}

fn jstrings(v: &[rust_jexl3::java::string::JString]) -> Json {
    Json::Arr(v.iter().map(|s| Json::Str(s.units().to_vec())).collect())
}

/// The node dump of Oracle.node(JexlNode).
fn node(n: NodeRef<'_>) -> Json {
    let mut attrs: Vec<(String, Json)> = Vec::new();
    if n.is_constant() {
        attrs.push(("const".into(), Json::Bool(true)));
    }
    if let Some(id) = n.identifier() {
        attrs.push(("name".into(), Json::str(id.get_name())));
        attrs.push(("symbol".into(), Json::Num(id.get_symbol().to_string())));
        if let Some(ns) = id.get_namespace() {
            attrs.push(("ns".into(), Json::str(ns)));
        }
        if id.is_redefined() {
            attrs.push(("redefined".into(), Json::Bool(true)));
        }
        if id.is_shaded() {
            attrs.push(("shaded".into(), Json::Bool(true)));
        }
        if id.is_captured() {
            attrs.push(("captured".into(), Json::Bool(true)));
        }
    } else if let Some(ia) = n.identifier_access() {
        attrs.push(("name".into(), Json::Str(ia.get_name().units().to_vec())));
        attrs.push(("id".into(), encode(&ia.get_identifier())));
        if n.is_safe() {
            attrs.push(("safe".into(), Json::Bool(true)));
        }
        if n.is_expression() {
            attrs.push(("expr".into(), Json::Bool(true)));
        }
    } else if let Some(num) = n.number() {
        attrs.push(("value".into(), encode(&num.get_literal_value())));
        attrs.push(("class".into(), Json::str(num.get_literal_class().expect("class").simple_name())));
        attrs.push(("image".into(), Json::str(&num.to_java_string())));
    } else if let Some(lit) = n.literal() {
        attrs.push(("value".into(), Json::Str(lit.units().to_vec())));
    } else if let Some(p) = n.regex() {
        attrs.push(("value".into(), Json::str(p.pattern())));
    } else if let Some(a) = n.annotation_name() {
        attrs.push(("name".into(), Json::str(a)));
    }
    if n.is_script() {
        let scope = n.get_scope();
        attrs.push(("args".into(), Json::Num(scope.map(|s| s.get_arg_count()).unwrap_or(0).to_string())));
        if let Some(s) = scope {
            attrs.push(("symbols".into(), strings(&s.get_symbols())));
            attrs.push(("params".into(), strings(&s.get_parameters())));
            attrs.push(("locals".into(), strings(&s.get_local_variables())));
            let caps: Vec<Json> = (0..s.get_symbols().len() as i32)
                .filter(|&i| s.is_captured_symbol(i))
                .map(|i| Json::Num(i.to_string()))
                .collect();
            attrs.push(("captured".into(), Json::Arr(caps)));
        }
        if let Some(p) = n.script().and_then(|s| s.get_pragmas()) {
            if !p.is_empty() {
                attrs.push(("pragmas".into(), encode(&Value::Map(engine::pragmas_as_map(p)))));
            }
        }
    }
    if n.symbol_count() > 0 {
        let ls: Vec<Json> = n
            .lexical_scope()
            .expect("scope")
            .symbols()
            .iter()
            .map(|s| Json::Num(s.to_string()))
            .collect();
        attrs.push(("lexical".into(), Json::Arr(ls)));
    }
    let kids: Vec<Json> = n.children().map(node).collect();
    Json::Arr(vec![
        Json::str(&n.class_name()),
        Json::Num(n.line().to_string()),
        Json::Num(n.column().to_string()),
        Json::Obj(attrs),
        Json::Arr(kids),
    ])
}

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

/// The engine configuration that matters to the parser: features, and the namespace test derived
/// from the engine's namespaces (Engine's constructor ORs `nsNames::contains` into it).
fn parse_features(engine: Option<&Json>) -> JexlFeatures {
    let mut f = features_of(engine.and_then(|e| e.get("features")));
    let mut ns: Vec<String> = Vec::new();
    if let Some(Json::Obj(kv)) = engine.and_then(|e| e.get("namespaces")) {
        ns.extend(kv.iter().map(|(k, _)| k.clone()));
    }
    if !ns.is_empty() {
        f = f.namespace_test(Some(std::sync::Arc::new(move |n: &str| ns.iter().any(|x| x == n))));
    }
    let script = engine
        .and_then(|e| e.get("features"))
        .and_then(|f| f.get("script"))
        .and_then(Json::as_bool);
    // Engine builds expressionFeatures/scriptFeatures from the same JexlFeatures
    f.script(script.unwrap_or(true))
}

fn run_case(parser: &mut Parser, case: &Json) -> Json {
    let src = case.get("src").and_then(Json::string).expect("src");
    let kind = case.get("kind").and_then(Json::string).unwrap_or_else(|| "script".into());
    let params: Option<Vec<String>> = case
        .get("params")
        .and_then(Json::arr)
        .map(|a| a.iter().filter_map(Json::string).collect());
    let mut features = parse_features(case.get("engine"));
    if kind == "expression" {
        features = features.script(false);
    }
    // Engine.createScript/createExpression trims the source before parsing
    let src = engine::trim_source(&src);
    let info = JexlInfo::new(Some("case".to_string()), 1, 1);
    let mut out: Vec<(String, Json)> = Vec::new();
    match parser.parse(Some(info), &features, &src, params.as_deref()) {
        Err(e) => {
            out.push((
                "parse".into(),
                Json::Obj(vec![
                    ("class".into(), Json::str(&e.class_name())),
                    ("msg".into(), e.get_message().map(|m| Json::Str(m.units().to_vec())).unwrap_or(Json::Null)),
                ]),
            ));
        }
        Ok(parsed) => {
            let Parsed { ast, root } = &parsed;
            let n = ast.node(*root);
            out.push(("ast".into(), node(n)));
            let mode = case
                .get("engine")
                .and_then(|e| e.get("collectMode"))
                .and_then(Json::string)
                .and_then(|s| s.parse::<i32>().ok())
                .unwrap_or(1);
            let vars: Vec<Json> = engine::get_variables_mode(&parsed, mode)
                .iter()
                .map(|v| jstrings(v))
                .collect();
            out.push(("vars".into(), Json::Arr(vars)));
            let scope = n.get_scope();
            out.push(("params".into(), strings(&scope.map(|s| s.get_parameters()).unwrap_or_default())));
            out.push(("locals".into(), strings(&scope.map(|s| s.get_local_variables()).unwrap_or_default())));
            let pragmas = n.script().and_then(|s| s.get_pragmas()).cloned().unwrap_or_default();
            out.push(("pragmas".into(), encode(&Value::Map(engine::pragmas_as_map(&pragmas)))));
        }
    }
    Json::Obj(out)
}

#[test]
fn parser_matches_oracle() {
    let cp = std::env::var("PARSER_CASES").unwrap_or_else(|_| "tests/data/parser/cases.jsonl".into());
    let ep = std::env::var("PARSER_EXPECTED").unwrap_or_else(|_| "tests/data/parser/expected.jsonl".into());
    let cases = std::fs::read_to_string(cp).expect("cases");
    let expected = std::fs::read_to_string(ep).expect("expected");
    let mut failures: Vec<String> = Vec::new();
    let mut parsers: std::collections::HashMap<String, Parser> = std::collections::HashMap::new();
    let mut n = 0usize;
    for (c, e) in cases.lines().zip(expected.lines()) {
        let case = json::parse(c).expect("case json");
        let want = json::parse(e).expect("expected json");
        n += 1;
        let key = case.get("engine").map(json::to_string).unwrap_or_default();
        let parser = parsers.entry(key).or_insert_with(Parser::new);
        let got = run_case(parser, &case);
        for field in ["parse", "ast", "vars", "params", "locals", "pragmas"] {
            let w = want.get(field);
            let g = got.get(field);
            // the oracle omits `pragmas` when the script has none
            if field == "pragmas" && w.is_none() {
                continue;
            }
            if w != g {
                if let Ok(path) = std::env::var("PARSER_DUMP") {
                    use std::io::Write;
                    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(path).expect("dump");
                    let rec = Json::Obj(vec![
                        ("id".into(), case.get("id").cloned().unwrap_or(Json::Null)),
                        ("src".into(), case.get("src").cloned().unwrap_or(Json::Null)),
                        ("field".into(), Json::str(field)),
                        ("want".into(), w.cloned().unwrap_or(Json::Null)),
                        ("got".into(), g.cloned().unwrap_or(Json::Null)),
                    ]);
                    writeln!(f, "{}", json::to_string(&rec)).expect("write");
                }
                failures.push(format!(
                    "{}: src={:?}\n  field {}\n  want {}\n  got  {}",
                    case.get("id").and_then(Json::string).unwrap_or_default(),
                    case.get("src").and_then(Json::string).unwrap_or_default(),
                    field,
                    w.map(json::to_string).unwrap_or_else(|| "-".into()),
                    g.map(json::to_string).unwrap_or_else(|| "-".into()),
                ));
                break;
            }
        }
    }
    assert!(n > 0, "no cases");
    assert!(
        failures.is_empty(),
        "{} of {} parser cases differ:\n{}",
        failures.len(),
        n,
        failures.iter().take(10).cloned().collect::<Vec<_>>().join("\n")
    );
}
