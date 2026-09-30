//! Execution differential test: runs the same scripts as the real Apache Commons JEXL 3.2.1 and
//! compares the parse outcome, `getVariables()`, the typed result, the exception (class and exact
//! message) and the context left behind.
mod common;

use std::collections::HashMap;
use std::sync::Arc;

use common::decode::decode;
use common::encode::encode;
use common::json::{self, Json};
use rust_jexl::jexl_context::{JexlContext, MapContext};
use rust_jexl::jexl_engine::{JexlBuilder, JexlEngine};
use rust_jexl::jexl_features::JexlFeatures;
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

fn strings(v: &[rust_jexl::java::string::JString]) -> Json {
    Json::Arr(v.iter().map(|s| Json::Str(s.units().to_vec())).collect())
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
    // the test host objects need an introspector, like a real embedder would register
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

fn run_case(case: &Json) -> Json {
    let src = case.get("src").and_then(Json::string).expect("src");
    let kind = case.get("kind").and_then(Json::string).unwrap_or_else(|| "script".into());
    let params: Option<Vec<String>> = case
        .get("params")
        .and_then(Json::arr)
        .map(|a| a.iter().filter_map(Json::string).collect());
    let args: Vec<Value> = case
        .get("args")
        .and_then(Json::arr)
        .map(|a| a.iter().map(decode).collect())
        .unwrap_or_default();
    let engine = build_engine(case.get("engine"));
    let context = Arc::new(MapContext::new());
    if let Some(Json::Obj(kv)) = case.get("ctx") {
        for (name, v) in kv {
            context.set(name, decode(v)).expect("bind");
        }
    }
    let mut out: Vec<(String, Json)> = Vec::new();
    // the oracle passes `new JexlInfo("case", 1, 1)`; the info shows up in every message
    let info = rust_jexl::jexl_info::JexlInfo::new(Some("case".to_string()), 1, 1);
    let script = if kind == "expression" {
        engine.create_expression(Some(info), &src)
    } else {
        engine.create_script_info(Some(info), &src, params.as_deref())
    };
    match script {
        Err(e) => {
            out.push(("parse".into(), error_json(&e)));
        }
        Ok(script) => {
            out.push(("vars".into(), Json::Arr(script.get_variables().iter().map(|v| strings(v)).collect())));
            match script.execute_args(context.clone(), &args) {
                Ok(v) => out.push(("result".into(), encode(&v))),
                Err(e) => out.push(("error".into(), error_json(&e))),
            }
            // the context, sorted by name like the oracle
            let mut entries = context.entries();
            entries.sort_by(|a, b| a.0.compare_to(&b.0).cmp(&0));
            out.push((
                "ctx".into(),
                Json::Obj(entries.iter().map(|(k, v)| (k.to_rust(), encode(v))).collect()),
            ));
        }
    }
    Json::Obj(out)
}

#[test]
fn execution_matches_oracle() {
    let cp = std::env::var("EXEC_CASES").unwrap_or_else(|_| "tests/data/exec/cases.jsonl".into());
    let ep = std::env::var("EXEC_EXPECTED").unwrap_or_else(|_| "tests/data/exec/expected.jsonl".into());
    eprintln!("reading fixtures");
    let cases = std::fs::read_to_string(cp).expect("cases");
    let expected = std::fs::read_to_string(ep).expect("expected");
    eprintln!("read {} / {} bytes", cases.len(), expected.len());
    let mut failures: Vec<String> = Vec::new();
    let mut n = 0usize;
    let mut skipped = 0usize;
    eprintln!("loop start");
    let mut iter = 0usize;
    for (c, e) in cases.lines().zip(expected.lines()) {
        iter += 1;
        if iter <= 40 {
            eprintln!("iter {} parsing", iter);
        }
        let case = json::parse(c).expect("case json");
        let want = json::parse(e).expect("expected json");
        if iter <= 40 {
            eprintln!("iter {} parsed", iter);
        }
        // scripts the oracle could not finish (infinite loops) carry no comparable outcome
        if want.get("timeout").is_some() || want.get("harness_error").is_some() {
            skipped += 1;
            continue;
        }
        n += 1;
        if std::env::var("EXEC_TRACE").is_ok() {
            eprintln!("case {} {}", n, case.get("id").and_then(Json::string).unwrap_or_default());
        }
        let got = common::normalize(&run_case(&case));
        for field in ["parse", "vars", "result", "error", "ctx"] {
            let w = want.get(field).map(common::normalize);
            let g = got.get(field).cloned();
            if w != g {
                if let Ok(path) = std::env::var("EXEC_DUMP") {
                    use std::io::Write;
                    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(path).expect("dump");
                    let rec = Json::Obj(vec![
                        ("id".into(), case.get("id").cloned().unwrap_or(Json::Null)),
                        ("src".into(), case.get("src").cloned().unwrap_or(Json::Null)),
                        ("field".into(), Json::str(field)),
                        ("want".into(), w.clone().unwrap_or(Json::Null)),
                        ("got".into(), g.clone().unwrap_or(Json::Null)),
                    ]);
                    writeln!(f, "{}", json::to_string(&rec)).expect("write");
                }
                failures.push(format!(
                    "{}: src={:?}\n  field {}\n  want {}\n  got  {}",
                    case.get("id").and_then(Json::string).unwrap_or_default(),
                    case.get("src").and_then(Json::string).unwrap_or_default(),
                    field,
                    w.map(|x| json::to_string(&x)).unwrap_or_else(|| "-".into()),
                    g.map(|x| json::to_string(&x)).unwrap_or_else(|| "-".into()),
                ));
                break;
            }
        }
    }
    assert!(n > 0, "no cases");
    assert!(
        failures.is_empty(),
        "{} of {} execution cases differ (skipped {}):\n{}",
        failures.len(),
        n,
        skipped,
        failures.iter().take(10).cloned().collect::<Vec<_>>().join("\n")
    );
}
