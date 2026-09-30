//! JXLT differential test: runs the same unified expressions and templates as the real Apache
//! Commons JEXL 3.2.1 `JxltEngine` and compares the parse outcome, `getVariables()`,
//! `getParameters()`, `getPragmas()`, `asString()`, the evaluated result / rendered output, the
//! exception (class and exact message) and the context left behind.
//!
//! Fixtures (`tools/gen_jxlt_cases.py`, replayed through the oracle with `tools/run_oracle.py`):
//!  * `tests/data/jxlt/cases.jsonl` / `expected.jsonl` — `kind: "jxlt"` and `kind: "template"`.
//!  * `tests/data/jxlt/api_cases.jsonl` / `api_expected.jsonl` — written by
//!    `tests/data/jxlt/gen/JxltGen.java`, which lives in the library's own package so it can
//!    reach what the oracle protocol cannot: `Expression.prepare()`, `Expression.getSource()`,
//!    `Expression.toString()`, `Template.toString()`, custom directive prefixes / expression
//!    characters, `createJxltEngine(noScript, cacheSize, immediate, deferred)` and
//!    `TemplateDebugger`.
mod common;

use std::collections::HashMap;
use std::sync::Arc;

use common::decode::decode;
use common::encode::encode;
use common::json::{self, Json};
use rust_jexl::internal::template_debugger::TemplateDebugger;
use rust_jexl::internal::template_interpreter::StringWriter;
use rust_jexl::java::string::JString;
use rust_jexl::jexl_context::{JexlContext, MapContext};
use rust_jexl::jexl_engine::{JexlBuilder, JexlEngine};
use rust_jexl::jexl_features::JexlFeatures;
use rust_jexl::jexl_info::JexlInfo;
use rust_jexl::jxlt_engine;
use rust_jexl::value::Value;

/// The exception shape Oracle.error() produces: class, message and one level of cause.
fn error_json(e: &rust_jexl::jexl_exception::JexlException) -> Json {
    let mut kv = vec![
        ("class".into(), Json::str(&e.class_name())),
        ("msg".into(), e.get_message().map(|m| Json::Str(m.units().to_vec())).unwrap_or(Json::Null)),
    ];
    if let Some(c) = e.get_cause() {
        kv.push((
            "cause".into(),
            Json::Obj(vec![
                ("class".into(), Json::str(&c.class_name())),
                ("msg".into(), c.get_message().map(|m| Json::Str(m.units().to_vec())).unwrap_or(Json::Null)),
            ]),
        ));
    }
    Json::Obj(kv)
}

fn strings(v: &[JString]) -> Json {
    Json::Arr(v.iter().map(|s| Json::Str(s.units().to_vec())).collect())
}

fn jstr(s: &JString) -> Json {
    Json::Str(s.units().to_vec())
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

fn build_engine(spec: Option<&Json>) -> Arc<JexlEngine> {
    let shim = rust_jexl::introspection::jdk_shim::JdkShim::new(rust_jexl::introspection::ResolverStrategy::Jexl)
        .with_hosts(Arc::new(common::hosts::TestHosts));
    let uber = rust_jexl::introspection::uberspect::Uberspect::new().with_shim(Arc::new(shim));
    let mut b = JexlBuilder::new().uberspect(Arc::new(uber));
    if let Some(Json::Obj(kv)) = spec {
        for (k, v) in kv {
            b = match k.as_str() {
                "strict" => b.strict(v.as_bool().unwrap_or(true)),
                "silent" => b.silent(v.as_bool().unwrap_or(false)),
                "safe" => b.safe(v.as_bool().unwrap_or(true)),
                "lexical" => b.lexical(v.as_bool().unwrap_or(false)),
                "lexicalShade" => b.lexical_shade(v.as_bool().unwrap_or(false)),
                "antish" => b.antish(v.as_bool().unwrap_or(true)),
                "cancellable" => b.cancellable(v.as_bool().unwrap_or(false)),
                "debug" => b.debug(v.as_bool().unwrap_or(true)),
                "collectMode" => b.collect_mode(v.string().and_then(|s| s.parse().ok()).unwrap_or(1)),
                "cache" => b.cache(v.string().and_then(|s| s.parse().ok()).unwrap_or(-1)),
                "cacheThreshold" => b.cache_threshold(v.string().and_then(|s| s.parse().ok()).unwrap_or(64)),
                "stackOverflow" => b.stack_overflow(v.string().and_then(|s| s.parse().ok()).unwrap_or(i32::MAX)),
                "namespaces" => {
                    let mut ns = HashMap::new();
                    if let Json::Obj(items) = v {
                        for (name, host) in items {
                            ns.insert(name.clone(), common::hosts::create(&host.string().unwrap_or_default()));
                        }
                    }
                    b.namespaces(ns)
                }
                "features" => b.features(features_of(Some(v))),
                "arithmetic" => {
                    let strict = v.get("strict").and_then(Json::as_bool).unwrap_or(true);
                    let mc = v.get("mathContext").and_then(Json::string);
                    let scale = v
                        .get("mathScale")
                        .and_then(Json::string)
                        .and_then(|s| s.parse::<i32>().ok())
                        .unwrap_or(i32::MIN);
                    b.arithmetic(rust_jexl::jexl_arithmetic::JexlArithmetic::new(
                        strict,
                        mc.map(|m| common::math_context(&m)),
                        scale,
                    ))
                }
                other => panic!("unknown engine option {}", other),
            };
        }
    }
    b.create()
}

fn context_of(case: &Json) -> Arc<MapContext> {
    let context = Arc::new(MapContext::new());
    if let Some(Json::Obj(kv)) = case.get("ctx") {
        for (name, v) in kv {
            context.set(name, decode(v)).expect("bind");
        }
    }
    context
}

fn ctx_json(context: &MapContext) -> Json {
    let mut entries = context.entries();
    entries.sort_by(|a, b| a.0.compare_to(&b.0).cmp(&0));
    Json::Obj(entries.iter().map(|(k, v)| (k.to_rust(), encode(v))).collect())
}

/// The source as UTF-16, exactly as the oracle handed it to Java.
fn src_of(case: &Json) -> JString {
    match case.get("src") {
        Some(Json::Str(u)) => JString::new(u.clone()),
        _ => JString::empty(),
    }
}

fn run_case(case: &Json) -> Json {
    let src = src_of(case);
    let kind = case.get("kind").and_then(Json::string).unwrap_or_else(|| "jxlt".into());
    let params: Option<Vec<String>> = case
        .get("params")
        .and_then(Json::arr)
        .map(|a| a.iter().filter_map(Json::string).collect());
    let args: Vec<Value> = case
        .get("args")
        .and_then(Json::arr)
        .map(|a| a.iter().map(decode).collect())
        .unwrap_or_default();
    let ops: Vec<String> = case
        .get("ops")
        .and_then(Json::arr)
        .map(|a| a.iter().filter_map(Json::string).collect())
        .unwrap_or_else(|| vec!["vars".into(), "exec".into()]);
    let engine = build_engine(case.get("engine"));
    let context = context_of(case);
    let info = JexlInfo::new(Some("case".to_string()), 1, 1);
    let jxlt = jxlt_engine::create_jxlt_engine(&engine);
    let mut out: Vec<(String, Json)> = Vec::new();
    if kind == "template" {
        let template = match jxlt.create_template(Some(info), "$$", &src, params.as_deref()) {
            Err(e) => {
                out.push(("parse".into(), error_json(&e)));
                return Json::Obj(out);
            }
            Ok(t) => t,
        };
        for op in &ops {
            match op.as_str() {
                "vars" => out.push(("vars".into(), Json::Arr(template.get_variables().iter().map(|v| strings(v)).collect()))),
                "params" => out.push((
                    "params".into(),
                    Json::Arr(template.get_parameters().iter().map(|p| Json::str(p)).collect()),
                )),
                "pragmas" => out.push(("pragmas".into(), encode(&template.get_pragmas()))),
                "parsed" => out.push(("parsed".into(), jstr(&template.as_string()))),
                "exec" => {
                    let writer = StringWriter::new();
                    match template.evaluate(context.clone(), Some(writer.clone()), &args) {
                        Ok(()) => out.push(("output".into(), jstr(&writer.to_jstring()))),
                        Err(e) => out.push(("error".into(), error_json(&e))),
                    }
                }
                "ctx" => out.push(("ctx".into(), ctx_json(&context))),
                other => panic!("unknown op {}", other),
            }
        }
    } else {
        let expr = match jxlt.create_expression(Some(info), &src) {
            Err(e) => {
                out.push(("parse".into(), error_json(&e)));
                return Json::Obj(out);
            }
            // a silent engine returns null; the oracle then NPEs and the case is skipped
            Ok(None) => return Json::Obj(vec![("harness_error".into(), Json::str("silent null"))]),
            Ok(Some(x)) => x,
        };
        for op in &ops {
            match op.as_str() {
                "vars" => out.push(("vars".into(), Json::Arr(expr.get_variables().iter().map(|v| strings(v)).collect()))),
                "parsed" => out.push(("parsed".into(), jstr(&expr.as_string()))),
                "exec" => match expr.evaluate(context.clone()) {
                    Ok(v) => out.push(("result".into(), encode(&v))),
                    Err(e) => out.push(("error".into(), error_json(&e))),
                },
                "ctx" => out.push(("ctx".into(), ctx_json(&context))),
                other => panic!("unknown op {}", other),
            }
        }
    }
    Json::Obj(out)
}

const FIELDS: [&str; 8] = ["parse", "vars", "params", "pragmas", "parsed", "result", "output", "error"];

fn replay(cases_path: &str, expected_path: &str, run: fn(&Json) -> Json, fields: &[&str]) -> (usize, usize, Vec<String>) {
    let cases = std::fs::read_to_string(cases_path).unwrap_or_else(|e| panic!("{}: {}", cases_path, e));
    let expected = std::fs::read_to_string(expected_path).unwrap_or_else(|e| panic!("{}: {}", expected_path, e));
    let mut failures: Vec<String> = Vec::new();
    let mut n = 0usize;
    let mut skipped = 0usize;
    for (c, e) in cases.lines().zip(expected.lines()) {
        let case = json::parse(c).expect("case json");
        let want = json::parse(e).expect("expected json");
        if want.get("timeout").is_some() || want.get("harness_error").is_some() {
            skipped += 1;
            continue;
        }
        n += 1;
        let got = common::normalize(&run(&case));
        if got.get("harness_error").is_some() {
            n -= 1;
            skipped += 1;
            continue;
        }
        for field in fields {
            let w = want.get(field).map(common::normalize);
            let g = got.get(field).cloned();
            if w != g {
                if let Ok(path) = std::env::var("JXLT_DUMP") {
                    use std::io::Write;
                    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(path).expect("dump");
                    let rec = Json::Obj(vec![
                        ("id".into(), case.get("id").cloned().unwrap_or(Json::Null)),
                        ("src".into(), case.get("src").cloned().unwrap_or(Json::Null)),
                        ("kind".into(), case.get("kind").cloned().unwrap_or(Json::Null)),
                        ("engine".into(), case.get("engine").cloned().unwrap_or(Json::Null)),
                        ("field".into(), Json::str(field)),
                        ("want".into(), w.clone().unwrap_or(Json::Null)),
                        ("got".into(), g.clone().unwrap_or(Json::Null)),
                    ]);
                    writeln!(f, "{}", json::to_string(&rec)).expect("write");
                }
                failures.push(format!(
                    "{}: kind={} src={:?}\n  field {}\n  want {}\n  got  {}",
                    case.get("id").and_then(Json::string).unwrap_or_default(),
                    case.get("kind").and_then(Json::string).unwrap_or_default(),
                    case.get("src").and_then(Json::string).unwrap_or_default(),
                    field,
                    w.map(|x| json::to_string(&x)).unwrap_or_else(|| "-".into()),
                    g.map(|x| json::to_string(&x)).unwrap_or_else(|| "-".into()),
                ));
                break;
            }
        }
    }
    (n, skipped, failures)
}

#[test]
fn jxlt_matches_oracle() {
    let cp = std::env::var("JXLT_CASES").unwrap_or_else(|_| "tests/data/jxlt/cases.jsonl".into());
    let ep = std::env::var("JXLT_EXPECTED").unwrap_or_else(|_| "tests/data/jxlt/expected.jsonl".into());
    let (n, skipped, failures) = replay(&cp, &ep, run_case, &FIELDS);
    assert!(n > 0, "no cases");
    assert!(
        failures.is_empty(),
        "{} of {} jxlt cases differ (skipped {}):\n{}",
        failures.len(),
        n,
        skipped,
        failures.iter().take(10).cloned().collect::<Vec<_>>().join("\n")
    );
}

// ------------------------------------------------------------------ the JxltGen.java API fixtures

/// One `api` case: the same source driven through the surface the oracle protocol cannot reach.
fn run_api_case(case: &Json) -> Json {
    let src = src_of(case);
    let kind = case.get("kind").and_then(Json::string).unwrap_or_else(|| "jxlt".into());
    let prefix = case.get("prefix").and_then(Json::string).unwrap_or_else(|| "$$".into());
    let immediate = case.get("immediate").and_then(Json::string).and_then(|s| s.chars().next()).unwrap_or('$');
    let deferred = case.get("deferred").and_then(Json::string).and_then(|s| s.chars().next()).unwrap_or('#');
    let noscript = case.get("noscript").and_then(Json::as_bool).unwrap_or(true);
    let cache_size = case.get("cacheSize").and_then(Json::string).and_then(|s| s.parse().ok()).unwrap_or(256);
    let params: Option<Vec<String>> = case
        .get("params")
        .and_then(Json::arr)
        .map(|a| a.iter().filter_map(Json::string).collect());
    let engine = build_engine(case.get("engine"));
    let context = context_of(case);
    let info = JexlInfo::new(Some("case".to_string()), 1, 1);
    let jxlt = jxlt_engine::create_jxlt_engine_with(&engine, noscript, cache_size, immediate, deferred);
    let mut out: Vec<(String, Json)> = Vec::new();
    if kind == "template" {
        let template = match jxlt.create_template(Some(info), &prefix, &src, params.as_deref()) {
            Err(e) => {
                out.push(("parse".into(), error_json(&e)));
                return Json::Obj(out);
            }
            Ok(t) => t,
        };
        out.push(("parsed".into(), jstr(&template.as_string())));
        out.push(("toString".into(), jstr(&template.java_to_jstring())));
        let mut dbg = TemplateDebugger::new();
        out.push(("debug".into(), Json::Bool(dbg.debug_template(&template))));
        out.push(("debugged".into(), jstr(&dbg.to_jstring())));
        match template.prepare(context.clone()) {
            Ok(None) => out.push(("prepared".into(), Json::Null)),
            Ok(Some(p)) => {
                out.push(("prepared".into(), jstr(&p.as_string())));
                let writer = StringWriter::new();
                match p.evaluate(context.clone(), Some(writer.clone()), &[]) {
                    Ok(()) => out.push(("output".into(), jstr(&writer.to_jstring()))),
                    Err(e) => out.push(("error".into(), error_json(&e))),
                }
            }
            Err(e) => out.push(("prepare_error".into(), error_json(&e))),
        }
    } else {
        let expr = match jxlt.create_expression(Some(info), &src) {
            Err(e) => {
                out.push(("parse".into(), error_json(&e)));
                return Json::Obj(out);
            }
            Ok(None) => return Json::Obj(vec![("harness_error".into(), Json::str("silent null"))]),
            Ok(Some(x)) => x,
        };
        out.push(("parsed".into(), jstr(&expr.as_string())));
        out.push(("toString".into(), jstr(&expr.java_to_jstring())));
        out.push(("immediate".into(), Json::Bool(expr.is_immediate())));
        out.push(("deferred".into(), Json::Bool(expr.is_deferred())));
        let mut dbg = TemplateDebugger::new();
        out.push(("debug".into(), Json::Bool(dbg.debug_expression(&expr))));
        out.push(("debugged".into(), jstr(&dbg.to_jstring())));
        match expr.prepare(context.clone()) {
            Ok(None) => out.push(("prepared".into(), Json::Null)),
            Ok(Some(p)) => {
                out.push(("prepared".into(), jstr(&p.as_string())));
                out.push(("prepared_toString".into(), jstr(&p.java_to_jstring())));
                out.push(("source".into(), jstr(&p.get_source().as_string())));
                match p.evaluate(context.clone()) {
                    Ok(v) => out.push(("result".into(), encode(&v))),
                    Err(e) => out.push(("error".into(), error_json(&e))),
                }
            }
            Err(e) => out.push(("prepare_error".into(), error_json(&e))),
        }
    }
    out.push(("ctx".into(), ctx_json(&context)));
    Json::Obj(out)
}

const API_FIELDS: [&str; 13] = [
    "parse",
    "parsed",
    "toString",
    "immediate",
    "deferred",
    "debug",
    "debugged",
    "prepared",
    "prepared_toString",
    "source",
    "result",
    "output",
    "error",
];

#[test]
fn jxlt_api_matches_oracle() {
    let cp = std::env::var("JXLT_API_CASES").unwrap_or_else(|_| "tests/data/jxlt/api_cases.jsonl".into());
    let ep = std::env::var("JXLT_API_EXPECTED").unwrap_or_else(|_| "tests/data/jxlt/api_expected.jsonl".into());
    let (n, skipped, failures) = replay(&cp, &ep, run_api_case, &API_FIELDS);
    assert!(n > 0, "no cases");
    assert!(
        failures.is_empty(),
        "{} of {} jxlt api cases differ (skipped {}):\n{}",
        failures.len(),
        n,
        skipped,
        failures.iter().take(10).cloned().collect::<Vec<_>>().join("\n")
    );
}
