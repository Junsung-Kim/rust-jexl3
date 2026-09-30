//! JexlArithmetic differential test: every arithmetic method against the real
//! org.apache.commons.jexl3.JexlArithmetic, over a value pool covering each Java type, null,
//! boundary numbers, numeric strings, Korean/emoji text and the collection kinds.
mod common;

use common::decode::decode;
use common::encode::encode;
use common::json::{self, Json};
use rust_jexl::jexl_arithmetic::{ArithError, JexlArithmetic};
use rust_jexl::java::big_decimal::MathContext;
use rust_jexl::value::Value;

fn math_context(spec: Option<&Json>) -> MathContext {
    match spec.and_then(Json::string) {
        None => MathContext::DECIMAL128,
        Some(s) => common::math_context(&s),
    }
}

fn arithmetic(spec: Option<&Json>) -> JexlArithmetic {
    let strict = spec.and_then(|s| s.get("strict")).and_then(Json::as_bool).unwrap_or(true);
    let mc = math_context(spec.and_then(|s| s.get("mathContext")));
    let scale = spec
        .and_then(|s| s.get("mathScale"))
        .and_then(Json::string)
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(i32::MIN);
    JexlArithmetic::new(strict, Some(mc), scale)
}

/// Runs one arithmetic probe; mirrors Oracle.arith.
fn run(a: &JexlArithmetic, op: &str, args: &[Value]) -> Result<Json, ArithError> {
    let v = match op {
        // unary
        "negate" => a.negate(&args[0])?,
        "positivize" => a.positivize(&args[0])?,
        "complement" => a.complement(&args[0])?,
        "not" => a.not(&args[0])?,
        "empty" => Value::Boolean(a.empty(&args[0])?),
        "isEmpty" => match a.is_empty(&args[0], Some(args[0].is_null()))? {
            Some(b) => Value::Boolean(b),
            None => Value::Null,
        },
        "size" => match a.size(&args[0], Some(if args[0].is_null() { 0 } else { 1 }))? {
            Some(i) => Value::Integer(i),
            None => Value::Null,
        },
        "toBoolean" => Value::Boolean(a.to_boolean(&args[0])?),
        "toInteger" => Value::Integer(a.to_integer(&args[0])?),
        "toLong" => Value::Long(a.to_long(&args[0])?),
        "toDouble" => Value::Double(a.to_double(&args[0])?),
        "toBigInteger" => Value::big_integer(a.to_big_integer(&args[0])?),
        "toBigDecimal" => Value::big_decimal(a.to_big_decimal(&args[0])?),
        "toString" => Value::String(a.to_jstring(&args[0])?),
        "narrow" => a.narrow(&args[0]),
        "isFloatingPointNumber" => Value::Boolean(a.is_floating_point_number(&args[0])),
        "isNumberable" => Value::Boolean(JexlArithmetic::is_numberable(&args[0])),
        "isFloatingPoint" => Value::Boolean(JexlArithmetic::is_floating_point(&args[0])),
        // binary
        "add" => a.add(&args[0], &args[1])?,
        "subtract" => a.subtract(&args[0], &args[1])?,
        "multiply" => a.multiply(&args[0], &args[1])?,
        "divide" => a.divide(&args[0], &args[1])?,
        "mod" => a.modulo(&args[0], &args[1])?,
        "and" => a.and(&args[0], &args[1])?,
        "or" => a.or(&args[0], &args[1])?,
        "xor" => a.xor(&args[0], &args[1])?,
        "equals" => Value::Boolean(a.equals(&args[0], &args[1])?),
        "lessThan" => Value::Boolean(a.less_than(&args[0], &args[1])?),
        "greaterThan" => Value::Boolean(a.greater_than(&args[0], &args[1])?),
        "lessThanOrEqual" => Value::Boolean(a.less_than_or_equal(&args[0], &args[1])?),
        "greaterThanOrEqual" => Value::Boolean(a.greater_than_or_equal(&args[0], &args[1])?),
        "contains" => match a.contains(&args[0], &args[1])? {
            Some(b) => Value::Boolean(b),
            None => Value::Null,
        },
        "startsWith" => match a.starts_with(&args[0], &args[1])? {
            Some(b) => Value::Boolean(b),
            None => Value::Null,
        },
        "endsWith" => match a.ends_with(&args[0], &args[1])? {
            Some(b) => Value::Boolean(b),
            None => Value::Null,
        },
        "createRange" => {
            let r = a.create_range(&args[0], &args[1])?;
            let values: Vec<Json> = r.iter().take(65).map(|v| encode(&v)).collect();
            return Ok(Json::Obj(vec![
                ("t".into(), Json::str("RangeValues")),
                ("c".into(), Json::str(&r.class_name())),
                ("v".into(), Json::Arr(values)),
            ]));
        }
        other => panic!("unknown op {}", other),
    };
    Ok(encode(&v))
}

#[test]
fn arithmetic_matches_oracle() {
    let cp = std::env::var("ARITH_CASES").unwrap_or_else(|_| "tests/data/arith/cases.jsonl".into());
    let ep = std::env::var("ARITH_EXPECTED").unwrap_or_else(|_| "tests/data/arith/expected.jsonl".into());
    let cases = std::fs::read_to_string(cp).expect("cases");
    let expected = std::fs::read_to_string(ep).expect("expected");
    let mut failures: Vec<String> = Vec::new();
    let mut n = 0usize;
    for (c, e) in cases.lines().zip(expected.lines()) {
        let case = json::parse(c).expect("case json");
        let outcome = json::parse(e).expect("expected json");
        // cases the oracle could not finish (a range that iterates forever) carry no result
        let want = match outcome.get("result") {
            Some(w) => w.clone(),
            None => continue,
        };
        n += 1;
        let op = case.get("op").and_then(Json::string).expect("op");
        let args: Vec<Value> = case.get("args").and_then(Json::arr).expect("args").iter().map(decode).collect();
        let a = arithmetic(case.get("arith"));
        let got = match run(&a, &op, &args) {
            Ok(v) => v,
            Err(err) => Json::Obj(vec![(
                "throw".into(),
                Json::Obj(vec![
                    ("class".into(), Json::str(err.class_name())),
                    (
                        "msg".into(),
                        match err.message() {
                            Some(m) => Json::Str(m.units().to_vec()),
                            None => Json::Null,
                        },
                    ),
                ]),
            )]),
        };
        // Identity hashes (`ClassName@1a2b3c`) are nondeterministic in Java: compare their shape.
        let want = common::normalize(&want);
        let got = common::normalize(&got);
        if got != want {
            if let Ok(path) = std::env::var("ARITH_DUMP") {
                use std::io::Write;
                let mut f = std::fs::OpenOptions::new().create(true).append(true).open(path).expect("dump");
                let rec = Json::Obj(vec![
                    ("op".into(), Json::str(&op)),
                    ("args".into(), case.get("args").cloned().unwrap_or(Json::Null)),
                    ("arith".into(), case.get("arith").cloned().unwrap_or(Json::Null)),
                    ("want".into(), want.clone()),
                    ("got".into(), got.clone()),
                ]);
                writeln!(f, "{}", json::to_string(&rec)).expect("write");
            }
            failures.push(format!(
                "{}: {}({})\n  want {}\n  got  {}",
                case.get("id").and_then(Json::string).unwrap_or_default(),
                op,
                case.get("args").map(json::to_string).unwrap_or_default(),
                json::to_string(&want),
                json::to_string(&got)
            ));
        }
    }
    assert!(n > 0, "no cases");
    assert!(
        failures.is_empty(),
        "{} of {} arithmetic cases differ:\n{}",
        failures.len(),
        n,
        failures.iter().take(12).cloned().collect::<Vec<_>>().join("\n")
    );
}
