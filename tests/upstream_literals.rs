//! The upstream Apache Commons JEXL 3.2.1 tests for literals, ranges and property/array access,
//! ported one method at a time.
//!
//! Sources: `ArrayAccessTest`, `ArrayLiteralTest`, `MapLiteralTest`, `SetLiteralTest`, `RangeTest`,
//! `internal/RangeTest`, `PropertyAccessTest` and `PublicFieldsTest` of
//! `~/src/commons-jexl-3.2.1/src/test/java/org/apache/commons/jexl3/`.
//! Every expectation here is one the JVM produces: the 148 methods of these eight classes were
//! built against `commons-jexl3-3.2.1.jar` and run green on Corretto 25 before being transcribed.
mod common;

use std::sync::Arc;

use common::upstream::*;
use rust_jexl::internal::debugger::Debugger;
use rust_jexl::internal::range::{Range, Width};
use rust_jexl::jexl_context::{JexlContext, MapContext};
use rust_jexl::jexl_engine::{empty_context, JexlEngine, JexlScript};
use rust_jexl::value::{Component, JArray, Value};

// ----------------------------------------------------------------------------------- local sugar

fn expression(jexl: &Arc<JexlEngine>, src: &str) -> JexlScript {
    jexl.create_expression(None, src).expect("createExpression")
}

fn script(jexl: &Arc<JexlEngine>, src: &str) -> JexlScript {
    jexl.create_script(src).expect("createScript")
}

fn script_of(jexl: &Arc<JexlEngine>, src: &str, params: &[&str]) -> JexlScript {
    let names: Vec<String> = params.iter().map(|p| (*p).to_string()).collect();
    jexl.create_script_named(src, &names).expect("createScript")
}

fn evaluate(e: &JexlScript, jc: &Arc<MapContext>) -> Value {
    e.evaluate(jc.clone() as Arc<dyn JexlContext>).expect("evaluate")
}

fn run(e: &JexlScript, jc: &Arc<MapContext>) -> Value {
    e.execute(jc.clone() as Arc<dyn JexlContext>).expect("execute")
}

/// `script.execute(null, args)` — Java's null context is `JexlEngine.EMPTY_CONTEXT`.
fn run_null(e: &JexlScript, args: &[Value]) -> Value {
    e.execute_args(empty_context(), args).expect("execute")
}

fn as_array(v: &Value) -> JArray {
    match v {
        Value::Array(a) => a.clone(),
        other => panic!("not an array: {}", other.class_name()),
    }
}

fn int_value(v: &Value) -> i32 {
    match v {
        Value::Byte(b) => *b as i32,
        Value::Short(s) => *s as i32,
        Value::Integer(n) => *n,
        Value::Long(n) => *n as i32,
        Value::Float(n) => *n as i32,
        Value::Double(n) => *n as i32,
        other => panic!("not a Number: {}", other.class_name()),
    }
}

fn long_value(v: &Value) -> i64 {
    match v {
        Value::Integer(n) => *n as i64,
        Value::Long(n) => *n,
        other => panic!("not a Number: {}", other.class_name()),
    }
}

fn as_range(v: &Value) -> &Range {
    v.as_host::<Range>().unwrap_or_else(|| panic!("not a range: {}", v.class_name()))
}

// =============================================================================== ArrayLiteralTest

// port of: ArrayLiteralTest.testEmptyArrayLiteral
#[test]
fn test_empty_array_literal() {
    let jexl = jexl();
    let jc = context();
    let o = evaluate(&expression(&jexl, "[]"), &jc);
    assert!(matches!(o, Value::Array(_)));
    assert_eq!(0, as_array(&o).len());
    let o = evaluate(&expression(&jexl, "[...]"), &jc);
    assert!(matches!(o, Value::List(_)));
    assert_eq!(0, match &o {
        Value::List(l) => l.len(),
        _ => unreachable!(),
    });
}

// port of: ArrayLiteralTest.testLiteralWithStrings
#[test]
fn test_literal_with_strings() {
    let jexl = jexl();
    let e = expression(&jexl, "[ 'foo' , 'bar' ]");
    let jc = context();

    let o = evaluate(&e, &jc);
    let check = [s("foo"), s("bar")];
    assert_array_eq(&check, &o);
}

// port of: ArrayLiteralTest.testLiteralWithElipsis
#[test]
fn test_literal_with_elipsis() {
    let jexl = jexl();
    let e = expression(&jexl, "[ 'foo' , 'bar', ... ]");
    let jc = context();

    let o = evaluate(&e, &jc);
    let check = vec![s("foo"), s("bar")];
    assert_java_eq(&list(check), &o);
    assert_eq!(2, match &o {
        Value::List(l) => l.len(),
        _ => panic!("not a List"),
    });
}

// port of: ArrayLiteralTest.testLiteralWithOneEntry
#[test]
fn test_literal_with_one_entry() {
    let jexl = jexl();
    let e = expression(&jexl, "[ 'foo' ]");
    let jc = context();

    let o = evaluate(&e, &jc);
    let check = [s("foo")];
    assert_array_eq(&check, &o);
}

// port of: ArrayLiteralTest.testLiteralWithNumbers
#[test]
fn test_literal_with_numbers() {
    let jexl = jexl();
    let e = expression(&jexl, "[ 5.0 , 10 ]");
    let jc = context();

    let o = evaluate(&e, &jc);
    let check = [d(5.0), i(10)];
    assert_array_eq(&check, &o);
    assert_eq!(Component::Class("java.lang.Number".into()), as_array(&o).component);
}

// port of: ArrayLiteralTest.testLiteralWithNulls
#[test]
fn test_literal_with_nulls() {
    let jexl = jexl();
    let exprs = ["[ null , 10 ]", "[ 10 , null ]", "[ 10 , null , 10]", "[ '10' , null ]", "[ null, '10' , null ]"];
    let checks: [Vec<Value>; 5] = [
        vec![Value::Null, i(10)],
        vec![i(10), Value::Null],
        vec![i(10), Value::Null, i(10)],
        vec![s("10"), Value::Null],
        vec![Value::Null, s("10"), Value::Null],
    ];
    let jc = context();
    for t in 0..exprs.len() {
        let e = expression(&jexl, exprs[t]);
        let o = evaluate(&e, &jc);
        assert_array_eq(&checks[t], &o);
    }
}

// port of: ArrayLiteralTest.testLiteralWithIntegers
#[test]
fn test_literal_with_integers() {
    let jexl = jexl();
    let e = expression(&jexl, "[ 5 , 10 ]");
    let jc = context();

    let o = evaluate(&e, &jc);
    assert_eq!(Component::Int, as_array(&o).component);
    assert_array_eq(&[i(5), i(10)], &o);
}

// port of: ArrayLiteralTest.testSizeOfSimpleArrayLiteral
#[test]
fn test_size_of_simple_array_literal() {
    let jexl = jexl();
    let e = expression(&jexl, "size([ 'foo' , 'bar' ])");
    let jc = context();

    let o = evaluate(&e, &jc);
    assert_java_eq(&i(2), &o);
}

// port of: ArrayLiteralTest.notestCallingMethodsOnNewMapLiteral
#[test]
fn notest_calling_methods_on_new_map_literal() {
    let jexl = jexl();
    let e = expression(&jexl, "size({ 'foo' : 'bar' }.values())");
    let jc = context();

    let o = evaluate(&e, &jc);
    assert_java_eq(&i(1), &o);
}

// port of: ArrayLiteralTest.testNotEmptySimpleArrayLiteral
#[test]
fn test_not_empty_simple_array_literal() {
    let jexl = jexl();
    let e = expression(&jexl, "empty([ 'foo' , 'bar' ])");
    let jc = context();

    let o = evaluate(&e, &jc);
    assert_java_eq(&Value::Boolean(false), &o);
}

// port of: ArrayLiteralTest.testChangeThroughVariables
#[test]
fn test_change_through_variables() {
    let jexl = jexl();
    let jc = context();
    let e147 = expression(&jexl, "quux = [one, two]");

    jc.set("one", i(1)).expect("set");
    jc.set("two", i(2)).expect("set");
    let o1 = as_array(&evaluate(&e147, &jc));
    assert_eq!(1, int_value(&o1.get(0).expect("[0]")));
    assert_eq!(2, int_value(&o1.get(1).expect("[1]")));

    jc.set("one", i(10)).expect("set");
    jc.set("two", i(20)).expect("set");
    let o2 = as_array(&evaluate(&e147, &jc));
    assert_eq!(10, int_value(&o2.get(0).expect("[0]")));
    assert_eq!(20, int_value(&o2.get(1).expect("[1]")));
}

// ================================================================================= SetLiteralTest

fn create_set(args: Vec<Value>) -> Value {
    hash_set(args)
}

// port of: SetLiteralTest.testSetLiteralWithStrings
#[test]
fn test_set_literal_with_strings() {
    let jexl = jexl();
    let e = expression(&jexl, "{ 'foo' , 'bar' }");
    let jc = context();

    let o = evaluate(&e, &jc);
    let check = create_set(vec![s("foo"), s("bar")]);
    assert_java_eq(&check, &o);
}

// port of: SetLiteralTest.testLiteralWithOneEntry
#[test]
fn test_set_literal_with_one_entry() {
    let jexl = jexl();
    let e = expression(&jexl, "{ 'foo' }");
    let jc = context();

    let o = evaluate(&e, &jc);
    let check = create_set(vec![s("foo")]);
    assert_java_eq(&check, &o);
}

// port of: SetLiteralTest.testSetLiteralWithStringsScript
#[test]
fn test_set_literal_with_strings_script() {
    let jexl = jexl();
    let e = script(&jexl, "{ 'foo' , 'bar' }");
    let jc = context();

    let o = run(&e, &jc);
    let check = create_set(vec![s("foo"), s("bar")]);
    assert_java_eq(&check, &o);
}

// port of: SetLiteralTest.testSetLiteralWithOneEntryScript
#[test]
fn test_set_literal_with_one_entry_script() {
    let jexl = jexl();
    let e = script(&jexl, "{ 'foo' }");
    let jc = context();

    let o = run(&e, &jc);
    let check = create_set(vec![s("foo")]);
    assert_java_eq(&check, &o);
}

// port of: SetLiteralTest.testSetLiteralWithOneEntryBlock
#[test]
fn test_set_literal_with_one_entry_block() {
    let jexl = jexl();
    let e = script(&jexl, "{ { 'foo' }; }");
    let jc = context();

    let o = run(&e, &jc);
    let check = create_set(vec![s("foo")]);
    assert_java_eq(&check, &o);
}

// port of: SetLiteralTest.testSetLiteralWithOneNestedSet
#[test]
fn test_set_literal_with_one_nested_set() {
    let jexl = jexl();
    let e = script(&jexl, "{ { 'foo' } }");
    let jc = context();

    let o = run(&e, &jc);
    let check = create_set(vec![create_set(vec![s("foo")])]);
    assert_java_eq(&check, &o);
}

// port of: SetLiteralTest.testSetLiteralWithNumbers
#[test]
fn test_set_literal_with_numbers() {
    let jexl = jexl();
    let e = expression(&jexl, "{ 5.0 , 10 }");
    let jc = context();

    let o = evaluate(&e, &jc);
    let check = create_set(vec![d(5.0), i(10)]);
    assert_java_eq(&check, &o);
}

// port of: SetLiteralTest.testSetLiteralWithNulls
#[test]
fn test_set_literal_with_nulls() {
    let jexl = jexl();
    let exprs = ["{  }", "{ 10 }", "{ 10 , null }", "{ 10 , null , 20}", "{ '10' , null }", "{ null, '10' , 20 }"];
    let checks = [
        create_set(vec![]),
        create_set(vec![i(10)]),
        create_set(vec![i(10), Value::Null]),
        create_set(vec![i(10), Value::Null, i(20)]),
        create_set(vec![s("10"), Value::Null]),
        create_set(vec![Value::Null, s("10"), i(20)]),
    ];
    let jc = context();
    for t in 0..exprs.len() {
        let e = script(&jexl, exprs[t]);
        let o = run(&e, &jc);
        assert_java_eq(&checks[t], &o);
    }
}

// port of: SetLiteralTest.testSizeOfSimpleSetLiteral
#[test]
fn test_size_of_simple_set_literal() {
    let jexl = jexl();
    let e = expression(&jexl, "size({ 'foo' , 'bar'})");
    let jc = context();

    let o = evaluate(&e, &jc);
    assert_java_eq(&i(2), &o);
}

// port of: SetLiteralTest.testNotEmptySimpleSetLiteral
#[test]
fn test_not_empty_simple_set_literal() {
    let jexl = jexl();
    let e = expression(&jexl, "empty({ 'foo' , 'bar' })");
    let jc = context();

    let o = evaluate(&e, &jc);
    assert_java_eq(&Value::Boolean(false), &o);
}

// ================================================================================= MapLiteralTest

// port of: MapLiteralTest.testLiteralWithStrings
#[test]
fn test_map_literal_with_strings() {
    let jexl = jexl();
    let e = expression(&jexl, "{ 'foo' : 'bar' }");
    let jc = context();

    let o = evaluate(&e, &jc);
    assert_java_eq(&hash_map(vec![(s("foo"), s("bar"))]), &o);
}

// port of: MapLiteralTest.testLiteralWithMultipleEntries
#[test]
fn test_literal_with_multiple_entries() {
    let jexl = jexl();
    let e = expression(&jexl, "{ 'foo' : 'bar', 'eat' : 'food' }");
    let jc = context();

    let expected = hash_map(vec![(s("foo"), s("bar")), (s("eat"), s("food"))]);

    let o = evaluate(&e, &jc);
    assert_java_eq(&expected, &o);
}

// port of: MapLiteralTest.testLiteralWithNumbers
#[test]
fn test_map_literal_with_numbers() {
    let jexl = jexl();
    let mut e = expression(&jexl, "{ 5 : 10 }");
    let jc = context();

    let mut o = evaluate(&e, &jc);
    assert_java_eq(&hash_map(vec![(i(5), i(10))]), &o);

    e = expression(&jexl, "m = { 3 : 30, 4 : 40, 5 : 'fifty', '7' : 'seven', 7 : 'SEVEN' }");
    evaluate(&e, &jc);

    e = expression(&jexl, "m.3");
    o = evaluate(&e, &jc);
    assert_java_eq(&i(30), &o);

    e = expression(&jexl, "m[4]");
    o = evaluate(&e, &jc);
    assert_java_eq(&i(40), &o);

    jc.set("i", i(5)).expect("set");
    e = expression(&jexl, "m[i]");
    o = evaluate(&e, &jc);
    assert_java_eq(&s("fifty"), &o);

    e = expression(&jexl, "m.3 = 'thirty'");
    evaluate(&e, &jc);
    e = expression(&jexl, "m.3");
    o = evaluate(&e, &jc);
    assert_java_eq(&s("thirty"), &o);

    e = expression(&jexl, "m['7']");
    o = evaluate(&e, &jc);
    assert_java_eq(&s("seven"), &o);

    e = expression(&jexl, "m.7");
    o = evaluate(&e, &jc);
    assert_java_eq(&s("SEVEN"), &o);

    jc.set("k", i(7)).expect("set");
    e = expression(&jexl, "m[k]");
    o = evaluate(&e, &jc);
    assert_java_eq(&s("SEVEN"), &o);

    jc.set("k", s("7")).expect("set");
    e = expression(&jexl, "m[k]");
    o = evaluate(&e, &jc);
    assert_java_eq(&s("seven"), &o);
}

// port of: MapLiteralTest.testSizeOfSimpleMapLiteral
#[test]
fn test_size_of_simple_map_literal() {
    let jexl = jexl();
    let e = expression(&jexl, "size({ 'foo' : 'bar' })");
    let jc = context();

    let o = evaluate(&e, &jc);
    assert_java_eq(&i(1), &o);
}

// port of: MapLiteralTest.testCallingMethodsOnNewMapLiteral
#[test]
fn test_calling_methods_on_new_map_literal() {
    let jexl = jexl();
    let e = expression(&jexl, "size({ 'foo' : 'bar' }.values())");
    let jc = context();

    let o = evaluate(&e, &jc);
    assert_java_eq(&i(1), &o);
}

// port of: MapLiteralTest.testNotEmptySimpleMapLiteral
#[test]
fn test_not_empty_simple_map_literal() {
    let jexl = jexl();
    let e = expression(&jexl, "empty({ 'foo' : 'bar' })");
    let jc = context();

    let o = evaluate(&e, &jc);
    assert_java_eq(&Value::Boolean(false), &o);
}

// port of: MapLiteralTest.testMapMapLiteral
#[test]
fn test_map_map_literal() {
    let jexl = jexl();
    let mut e = expression(&jexl, "{'foo' : { 'inner' : 'bar' }}");
    let jc = context();
    let mut o = evaluate(&e, &jc);
    assert!(!o.is_null());

    jc.set("outer", o).expect("set");
    e = expression(&jexl, "outer.foo.inner");
    o = evaluate(&e, &jc);
    assert_java_eq(&s("bar"), &o);
}

// port of: MapLiteralTest.testMapArrayLiteral
#[test]
fn test_map_array_literal() {
    let jexl = jexl();
    let mut e = expression(&jexl, "{'foo' : [ 'inner' , 'bar' ]}");
    let jc = context();
    let mut o = evaluate(&e, &jc);
    assert!(!o.is_null());

    jc.set("outer", o).expect("set");
    e = expression(&jexl, "outer.foo.1");
    o = evaluate(&e, &jc);
    assert_java_eq(&s("bar"), &o);
}

// port of: MapLiteralTest.testEmptyMap
#[test]
fn test_empty_map() {
    let jexl = jexl();
    let script = script_of(&jexl, "map['']", &["map"]);
    let result = run_null(&script, &[hash_map(vec![(s(""), i(42))])]);
    assert_java_eq(&i(42), &result);
}

// port of: MapLiteralTest.testVariableMap
#[test]
fn test_variable_map() {
    let jexl = jexl();
    let script = script_of(&jexl, "{ ['1', '2'.toString()] : someValue }", &["someValue"]);
    let result = run_null(&script, &[i(42)]);
    assert!(matches!(result, Value::Map(_)));
    let map = match &result {
        Value::Map(m) => m.clone(),
        _ => unreachable!(),
    };
    let (key, value) = map.snapshot().into_iter().next().expect("one entry");
    let gg = map.get(&key).expect("get(key)");
    assert_eq!(42, int_value(&gg));
    assert_eq!(int_value(&value), int_value(&gg));
}

// ===================================================================================== RangeTest

// port of: RangeTest.testIntegerRangeOne
#[test]
fn test_integer_range_one() {
    let jexl = jexl();
    let e = expression(&jexl, "(1..1)");
    let jc = context();

    let o = evaluate(&e, &jc);
    assert!(o.as_host::<Range>().is_some());
    let c = as_range(&o);
    assert_eq!(1, c.size());
    let a: Vec<Value> = c.iter().collect();
    assert_eq!(1, a.len());
    assert_eq!(1, int_value(&a[0]));
    // Java passes the JexlExpression itself; a closure is this port's script-as-a-value
    let expr_value = run_null(&script(&jexl, "()->{ 1 }"), &[]);
    assert_java_eq(&Value::Boolean(false), &run_null(&script_of(&jexl, "empty x", &["x"]), &[expr_value]));
}

// port of: RangeTest.testIntegerRange
#[test]
fn test_integer_range() {
    let jexl = jexl();
    let e = expression(&jexl, "(1..32)");
    let jc = context();

    let o0 = evaluate(&e, &jc);
    let o = evaluate(&e, &jc);
    assert!(o.as_host::<Range>().is_some());
    let c = as_range(&o);
    assert_eq!(32, c.size());

    assert!(!o0.same_instance(&o));
    assert_eq!(o0.java_hash_code(), o.java_hash_code());
    assert_java_eq(&o0, &o);

    let mut n = 0;
    for v in c.iter() {
        n += 1;
        assert_eq!(n, int_value(&v));
    }
    assert_eq!(32, n);

    let oaa: Vec<Value> = c.iter().collect();
    assert_eq!(32, oaa.len());
    for l in 0..32 {
        assert_java_eq(&oaa[l], &i(l as i32 + 1));
    }
}

// port of: RangeTest.testLongRange
#[test]
fn test_long_range() {
    let jexl = jexl();
    let e = expression(&jexl, "(6789000001L..6789000032L)");
    let jc = context();

    let o0 = evaluate(&e, &jc);
    let o = evaluate(&e, &jc);
    assert!(o.as_host::<Range>().is_some());
    let c = as_range(&o);
    assert_eq!(32, c.size());
    let expr_value = run_null(&script(&jexl, "()->{ 1 }"), &[]);
    assert_java_eq(&Value::Boolean(false), &run_null(&script_of(&jexl, "empty x", &["x"]), &[expr_value]));

    assert!(!o0.same_instance(&o));
    assert_eq!(o0.java_hash_code(), o.java_hash_code());
    assert_java_eq(&o0, &o);

    let mut n: i64 = 6789000000;
    for v in c.iter() {
        n += 1;
        assert_eq!(n, long_value(&v));
    }
    assert_eq!(6789000032i64, n);

    let oaa: Vec<Value> = c.iter().collect();
    assert_eq!(32, oaa.len());
    for k in 0..32 {
        assert_java_eq(&oaa[k], &l(6789000001 + k as i64));
    }
}

// port of: RangeTest.testIntegerSum
#[test]
fn test_integer_sum() {
    let jexl = jexl();
    let e = script(&jexl, "var s = 0; for(var i : (1..5)) { s = s + i; }; s");
    let jc = context();

    let o = run(&e, &jc);
    assert_eq!(15, int_value(&o));
}

// port of: RangeTest.testIntegerContains
#[test]
fn test_integer_contains() {
    let jexl = jexl();
    let e = script(&jexl, "(x)->{ x =~ (1..10) }");
    let jc = context();

    let mut o = e.execute_args(jc.clone() as Arc<dyn JexlContext>, &[i(5)]).expect("execute");
    assert_java_eq(&Value::Boolean(true), &o);
    o = e.execute_args(jc.clone() as Arc<dyn JexlContext>, &[i(0)]).expect("execute");
    assert_java_eq(&Value::Boolean(false), &o);
    o = e.execute_args(jc.clone() as Arc<dyn JexlContext>, &[i(100)]).expect("execute");
    assert_java_eq(&Value::Boolean(false), &o);
}

// port of: RangeTest.testLongSum
#[test]
fn test_long_sum() {
    let jexl = jexl();
    let e = script(&jexl, "var s = 0; for(var i : (6789000001L..6789000001L)) { s = s + i; }; s");
    let jc = context();

    let o = run(&e, &jc);
    assert_eq!(6789000001i64, long_value(&o));
}

// port of: RangeTest.testLongContains
#[test]
fn test_long_contains() {
    let jexl = jexl();
    let e = script(&jexl, "(x)->{ x =~ (90000000001L..90000000010L) }");
    let jc = context();

    let mut o = e.execute_args(jc.clone() as Arc<dyn JexlContext>, &[l(90000000005)]).expect("execute");
    assert_java_eq(&Value::Boolean(true), &o);
    o = e.execute_args(jc.clone() as Arc<dyn JexlContext>, &[i(0)]).expect("execute");
    assert_java_eq(&Value::Boolean(false), &o);
    o = e.execute_args(jc.clone() as Arc<dyn JexlContext>, &[l(90000000011)]).expect("execute");
    assert_java_eq(&Value::Boolean(false), &o);
}

// ============================================================================ internal/RangeTest

fn check_iteration(r: &Range, first: i64, last: i64) {
    let mut it = r.iter();
    let mut v = long_value(&it.next().expect("empty iterator?"));
    assert_eq!(first, v);
    for next in it {
        v = long_value(&next);
    }
    assert_eq!(last, v);
}

fn contains_all(a: &Range, b: &Range) -> bool {
    b.iter().all(|v| a.contains(&v))
}

// port of: internal.RangeTest.testRanges
//
// The Collection mutators (`add`, `remove`, `addAll`, `removeAll`, `retainAll`, `Iterator.remove`)
// and the `NoSuchElementException` past the end have no counterpart: the ported `Range` exposes a
// Rust iterator and no mutating API, so those assertions are reported, not transcribed.
#[test]
fn test_ranges() {
    let lr0 = Range::create(Width::Long, 20, 10);
    assert_eq!(10i64, long_value(&lr0.get_min()));
    assert_eq!(20i64, long_value(&lr0.get_max()));
    assert!(lr0.size() != 0);
    assert!(lr0.contains(&l(10)));
    assert!(lr0.contains(&l(20)));
    assert!(!lr0.contains(&l(30)));
    assert!(!lr0.contains(&l(5)));
    assert!(!lr0.contains(&Value::Null));
    check_iteration(&lr0, 20, 10);
    let lr1 = Range::create(Width::Long, 10, 20);
    check_iteration(&lr1, 10, 20);
    assert!(contains_all(&lr0, &lr1));
    let lr2 = Range::create(Width::Long, 10, 15);
    assert!(!Value::object(lr0.clone()).java_equals(&Value::object(lr2.clone())));
    assert!(contains_all(&lr0, &lr2));
    assert!(!contains_all(&lr2, &lr1));
    let ir0 = Range::create(Width::Integer, 20, 10);
    check_iteration(&ir0, 20, 10);
    assert_eq!(10, int_value(&ir0.get_min()));
    assert_eq!(20, int_value(&ir0.get_max()));
    assert!(ir0.size() != 0);
    assert!(ir0.contains(&i(10)));
    assert!(ir0.contains(&i(20)));
    assert!(!ir0.contains(&i(30)));
    assert!(!ir0.contains(&i(5)));
    assert!(!ir0.contains(&Value::Null));
    let ir1 = Range::create(Width::Integer, 10, 20);
    check_iteration(&ir1, 10, 20);
    assert!(contains_all(&ir0, &ir1));
    assert!(!Value::object(ir0.clone()).java_equals(&Value::object(lr0.clone())));
    assert!(!Value::object(ir1.clone()).java_equals(&Value::object(lr1.clone())));
    let ir2 = Range::create(Width::Integer, 10, 15);
    assert!(!Value::object(ir0.clone()).java_equals(&Value::object(ir2.clone())));
    assert!(contains_all(&ir0, &ir2));
    assert!(!contains_all(&ir2, &ir1));

    let mut lc0: i64 = 20;
    for v0 in lr0.iter() {
        assert_eq!(lc0, long_value(&v0));
        lc0 -= 1;
    }
    assert_eq!(9i64, lc0);

    let mut ic0: i32 = 20;
    for v0 in ir0.iter() {
        assert_eq!(ic0, int_value(&v0));
        ic0 -= 1;
    }
    assert_eq!(9, ic0);
}

// ================================================================================ ArrayAccessTest

// port of: ArrayAccessTest.testArrayAccess
#[test]
fn test_array_access() {
    let asserter = Asserter::new(jexl());

    // test List access
    let l = list(vec![i(1), i(2), i(3)]);
    asserter.set_variable("list", l);

    asserter.assert_expression("list[1]", &i(2));
    asserter.assert_expression("list[1+1]", &i(3));
    asserter.set_variable("loc", i(1));
    asserter.assert_expression("list[loc+1]", &i(3));

    // test array access
    let args = string_array(&["hello", "there"]);
    asserter.set_variable("array", args);
    asserter.assert_expression("array[0]", &s("hello"));

    // to think that this was an intentional syntax...
    asserter.assert_expression("array.0", &s("hello"));

    // test map access
    let m = hash_map(vec![(s("foo"), s("bar"))]);
    asserter.set_variable("map", m);
    asserter.set_variable("key", s("foo"));

    asserter.assert_expression("map[\"foo\"]", &s("bar"));
    asserter.assert_expression("map[key]", &s("bar"));

    // test bean access
    asserter.set_variable("foo", Foo::new());
    asserter.assert_expression("foo[\"bar\"]", &s(GET_METHOD_STRING));
    asserter.assert_expression("foo[\"bar\"] == foo.bar", &Value::Boolean(true));
}

// port of: ArrayAccessTest.testDoubleArrays
#[test]
fn test_double_arrays() {
    let asserter = Asserter::new(jexl());
    let row = object_array(vec![Value::Null, Value::Null]);
    let foo = Value::Array(JArray::new(
        Component::Class("[Ljava.lang.Object;".into()),
        vec![row.clone(), object_array(vec![Value::Null, Value::Null])],
    ));
    let reset = || {
        as_array(&row).set(0, s("one"));
        as_array(&row).set(1, s("two"));
    };

    reset();
    asserter.set_variable("foo", foo);
    asserter.assert_expression("foo[0][1]", &s("two"));
    asserter.assert_expression("foo[0][1] = 'three'", &s("three"));
    asserter.assert_expression("foo[0][1]", &s("three"));

    reset();
    asserter.assert_expression("foo.0[1]", &s("two"));
    asserter.assert_expression("foo.0[1] = 'three'", &s("three"));
    asserter.assert_expression("foo.0[1]", &s("three"));

    reset();
    asserter.assert_expression("foo.0.'1'", &s("two"));
    asserter.assert_expression("foo.0.'1' = 'three'", &s("three"));
    asserter.assert_expression("foo.0.'1'", &s("three"));

    reset();
    asserter.assert_expression("foo.'0'.'1'", &s("two"));
    asserter.assert_expression("foo.'0'.'1' = 'three'", &s("three"));
    asserter.assert_expression("foo.'0'.'1'", &s("three"));

    reset();
    asserter.assert_expression("foo.0.1", &s("two"));
    asserter.assert_expression("foo.0.1 = 'three'", &s("three"));
    asserter.assert_expression("foo.0.1", &s("three"));
}

// port of: ArrayAccessTest.testDoubleMaps
#[test]
fn test_double_maps() {
    let asserter = Asserter::new(jexl());
    let foo0 = hash_map(vec![(i(0), s("one")), (i(1), s("two")), (s("3.0"), s("three"))]);
    let foo = hash_map(vec![(i(0), foo0.clone())]);
    let reset = || {
        if let Value::Map(m) = &foo0 {
            m.put(i(0), s("one"));
            m.put(i(1), s("two"));
        }
    };

    asserter.set_variable("foo", foo);
    asserter.assert_expression("foo[0][1]", &s("two"));
    asserter.assert_expression("foo[0][1] = 'three'", &s("three"));
    asserter.assert_expression("foo[0][1]", &s("three"));
    asserter.assert_expression("foo[0]['3.0']", &s("three"));

    reset();
    asserter.assert_expression("foo.0[1]", &s("two"));
    asserter.assert_expression("foo.0[1] = 'three'", &s("three"));
    asserter.assert_expression("foo.0[1]", &s("three"));
    asserter.assert_expression("foo.0['3.0']", &s("three"));

    reset();
    asserter.assert_expression("foo.0.'1'", &s("two"));
    asserter.assert_expression("foo.0.'1' = 'three'", &s("three"));
    asserter.assert_expression("foo.0.'1'", &s("three"));

    reset();
    asserter.assert_expression("foo.'0'.'1'", &s("two"));
    asserter.assert_expression("foo.'0'.'1' = 'three'", &s("three"));
    asserter.assert_expression("foo.'0'.'1'", &s("three"));

    reset();
    asserter.assert_expression("foo.0.1", &s("two"));
    asserter.assert_expression("foo.0.1 = 'three'", &s("three"));
    asserter.assert_expression("foo.0.1", &s("three"));
}

// port of: ArrayAccessTest.testArrayProperty
#[test]
fn test_array_property() {
    let asserter = Asserter::new(jexl());
    let foo = Foo::new();

    asserter.set_variable("foo", foo);

    asserter.assert_expression("foo.array[1]", &s(GET_METHOD_ARRAY[1]));
    asserter.assert_expression("foo.array.1", &s(GET_METHOD_ARRAY[1]));
    asserter.assert_expression("foo.array2[1][1]", &s(GET_METHOD_ARRAY2[1][1]));
    asserter.assert_expression("foo.array2[1].1", &s(GET_METHOD_ARRAY2[1][1]));
}

// port of: ArrayAccessTest.testArrayAndDottedConflict  (JEXL-26)
#[test]
fn test_array_and_dotted_conflict() {
    let asserter = Asserter::new(jexl());
    let objects = object_array(vec![s("an"), s("array"), l(0)]);
    asserter.set_strict(false);
    asserter.set_silent(true);
    asserter.set_variable("objects", objects);
    asserter.set_variable("status", s("Enabled"));
    asserter.assert_expression("objects[1].status", &Value::Null);
    asserter.assert_expression("objects.1.status", &Value::Null);

    asserter.set_variable("base.status", s("Ok"));
    asserter.assert_expression("base.objects[1].status", &Value::Null);
    asserter.assert_expression("base.objects.1.status", &Value::Null);
}

// port of: ArrayAccessTest.testArrayIdentifierParsing
#[test]
fn test_array_identifier_parsing() {
    let asserter = Asserter::new(jexl());
    let map = hash_map(vec![(s("00200"), d(-42.42)), (i(200), d(42.42))]);
    asserter.set_variable("objects", map);
    asserter.assert_expression("objects.get('00200')", &d(-42.42));
    asserter.assert_expression("objects.'00200'", &d(-42.42));
    asserter.assert_expression("objects.get(200)", &d(42.42));
    asserter.assert_expression("objects.'200'", &d(42.42));
    asserter.assert_expression("objects.200", &d(42.42));
}

// port of: ArrayAccessTest.testArrayMethods
#[test]
fn test_array_methods() {
    let asserter = Asserter::new(jexl());
    let objects = object_array(vec![s("an"), s("array"), l(0)]);

    asserter.set_variable("objects", objects);
    asserter.assert_expression("objects.get(1)", &s("array"));
    asserter.assert_expression("objects.size()", &i(3));
    // setting an index returns the old value
    asserter.assert_expression("objects.set(1, 'dion')", &s("array"));
    asserter.assert_expression("objects[1]", &s("dion"));
}

// port of: ArrayAccessTest.testArrayArray
#[test]
fn test_array_array() {
    let asserter = Asserter::new(jexl());
    let i42 = i(42);
    let i43 = i(43);
    let s42 = s("fourty-two");
    let s43 = s("fourty-three");
    let foo = object_array(vec![Value::Null, i42.clone(), s42.clone()]);
    as_array(&foo).set(0, foo.clone());
    let foo1 = as_array(&foo).get(1).expect("[1]");
    let foo2 = as_array(&foo).get(2).expect("[2]");
    asserter.set_variable("foo", foo.clone());
    asserter.set_variable("zero", i(0));
    asserter.set_variable("one", i(1));
    asserter.set_variable("two", i(2));
    for _ in 0..2 {
        asserter.assert_expression("foo[0]", &foo);
        asserter.assert_expression("foo[0][0]", &foo);
        asserter.assert_expression("foo[1]", &foo1);
        asserter.assert_expression("foo[0][1]", &foo1);
        asserter.assert_expression("foo[0][1] = 43", &i43);
        asserter.assert_expression("foo[0][1]", &i43);
        asserter.assert_expression("foo[0][1] = 42", &i42);
        asserter.assert_expression("foo[0][1]", &i42);
        asserter.assert_expression("foo[0][0][1]", &foo1);
        asserter.assert_expression("foo[0][0][1] = 43", &i43);
        asserter.assert_expression("foo[0][0][1]", &i43);
        asserter.assert_expression("foo[0][0][1] = 42", &i42);
        asserter.assert_expression("foo[0][0][1]", &i42);
        asserter.assert_expression("foo[2]", &foo2);
        asserter.assert_expression("foo[0][2]", &foo2);
        asserter.assert_expression("foo[0][2] = 'fourty-three'", &s43);
        asserter.assert_expression("foo[0][2]", &s43);
        asserter.assert_expression("foo[0][2] = 'fourty-two'", &s42);
        asserter.assert_expression("foo[0][2]", &s42);
        asserter.assert_expression("foo[0][0][2]", &foo2);
        asserter.assert_expression("foo[0][0][2] = 'fourty-three'", &s43);
        asserter.assert_expression("foo[0][0][2]", &s43);
        asserter.assert_expression("foo[0][0][2] = 'fourty-two'", &s42);
        asserter.assert_expression("foo[0][0][2]", &s42);

        asserter.assert_expression("foo[zero]", &foo);
        asserter.assert_expression("foo[zero][zero]", &foo);
        asserter.assert_expression("foo[one]", &foo1);
        asserter.assert_expression("foo[zero][one]", &foo1);
        asserter.assert_expression("foo[zero][one] = 43", &i43);
        asserter.assert_expression("foo[zero][one]", &i43);
        asserter.assert_expression("foo[zero][one] = 42", &i42);
        asserter.assert_expression("foo[zero][one]", &i42);
        asserter.assert_expression("foo[zero][zero][one]", &foo1);
        asserter.assert_expression("foo[zero][zero][one] = 43", &i43);
        asserter.assert_expression("foo[zero][zero][one]", &i43);
        asserter.assert_expression("foo[zero][zero][one] = 42", &i42);
        asserter.assert_expression("foo[zero][zero][one]", &i42);
        asserter.assert_expression("foo[two]", &foo2);
        asserter.assert_expression("foo[zero][two]", &foo2);
        asserter.assert_expression("foo[zero][two] = 'fourty-three'", &s43);
        asserter.assert_expression("foo[zero][two]", &s43);
        asserter.assert_expression("foo[zero][two] = 'fourty-two'", &s42);
        asserter.assert_expression("foo[zero][two]", &s42);
        asserter.assert_expression("foo[zero][zero][two]", &foo2);
        asserter.assert_expression("foo[zero][zero][two] = 'fourty-three'", &s43);
        asserter.assert_expression("foo[zero][zero][two]", &s43);
        asserter.assert_expression("foo[zero][zero][two] = 'fourty-two'", &s42);
        asserter.assert_expression("foo[zero][zero][two]", &s42);
    }
}

// port of: ArrayAccessTest.testArrayGetSet
#[test]
fn test_array_get_set() {
    let asserter = Asserter::new(jexl());
    let bar = Sample::new(int_array(&[24]));
    asserter.set_variable("bar", bar);
    asserter.assert_expression("bar.foo[0]", &i(24));
    asserter.assert_expression("bar.foo = []", &object_array(vec![]));
}

// ============================================================================= PropertyAccessTest

// port of: PropertyAccessTest.testPropertyProperty
#[test]
fn test_property_property() {
    let asserter = Asserter::new(jexl());
    let i42 = i(42);
    let i43 = i(43);
    let s42 = s("fourty-two");
    let foo = object_array(vec![Value::Null, i42.clone(), s42.clone()]);
    as_array(&foo).set(0, foo.clone());
    let foo1 = as_array(&foo).get(1).expect("[1]");
    asserter.set_variable("foo", foo.clone());
    asserter.set_variable("zero", i(0));
    asserter.set_variable("one", i(1));
    asserter.set_variable("two", i(2));
    for _ in 0..2 {
        asserter.assert_expression("foo.0", &foo);
        asserter.assert_expression("foo.0.'0'", &foo);
        asserter.assert_expression("foo.'1'", &foo1);
        asserter.assert_expression("foo.0.'1'", &foo1);
        asserter.assert_expression("foo.0.'1' = 43", &i43);
        asserter.assert_expression("foo.0.'1'", &i43);
        asserter.assert_expression("foo.0.'1' = 42", &i42);
        //
        asserter.assert_expression("foo?.0.'1'", &i42);
        asserter.assert_expression("foo?.0", &foo);
        asserter.assert_expression("foo?.0.'0'", &foo);
        asserter.assert_expression("foo?.'1'", &foo1);
        asserter.assert_expression("foo.0?.'1'", &foo1);
        asserter.assert_expression("foo?.0.'1' = 43", &i43);
        asserter.assert_expression("foo?.0?.'1'", &i43);
        asserter.assert_expression("foo?.0.'1' = 42", &i42);
        asserter.assert_expression("foo?.0.'1'", &i42);
        //
        asserter.assert_expression("foo?.0.`1`", &i42);
        asserter.assert_expression("foo?.0", &foo);
        asserter.assert_expression("foo?.0.'0'", &foo);
        asserter.assert_expression("foo?.`1`", &foo1);
        asserter.assert_expression("foo?.0.`1`", &foo1);
        asserter.assert_expression("foo?.0.`${one}` = 43", &i43);
        asserter.assert_expression("foo.0?.`${one}`", &i43);
        asserter.assert_expression("foo.0.`${one}` = 42", &i42);
        asserter.assert_expression("foo?.0?.`${one}`", &i42);
    }
}

// port of: PropertyAccessTest.testStringIdentifier
#[test]
fn test_string_identifier() {
    let jexl = jexl();
    let foo = hash_map(vec![(s("q u u x"), s("456"))]);

    let jc = context();
    jc.set("foo", foo).expect("set");
    let e = expression(&jexl, "foo.\"q u u x\"");
    let mut result = evaluate(&e, &jc);
    assert_java_eq(&s("456"), &result);
    let e2 = expression(&jexl, "foo.'q u u x'");
    result = evaluate(&e2, &jc);
    assert_java_eq(&s("456"), &result);
    let mut sc = script(&jexl, "foo.\"q u u x\"");
    result = run(&sc, &jc);
    assert_java_eq(&s("456"), &result);
    sc = script(&jexl, "foo.'q u u x'");
    result = run(&sc, &jc);
    assert_java_eq(&s("456"), &result);

    let mut dbg = Debugger::new();
    let dbgdata = dbg.data(e2.parsed().node()).to_rust();
    assert_eq!("foo.'q u u x'", dbgdata);
}

// port of: PropertyAccessTest.testErroneousIdentifier
#[test]
fn test_erroneous_identifier() {
    let ctx = context();
    let engine = builder().strict(true).silent(false).create();

    // base succeeds
    let mut stmt = "(x)->{ x?.class ?? 'oops' }";
    let mut sc = script(&engine, stmt);
    let mut result = sc.execute_args(ctx.clone() as Arc<dyn JexlContext>, &[s("querty")]).expect("execute");
    assert_java_eq(&class_of("java.lang.String"), &result);

    // fail with unknown property
    stmt = "(x)->{ x.class1 ?? 'oops' }";
    sc = script(&engine, stmt);
    result = sc.execute_args(ctx.clone() as Arc<dyn JexlContext>, &[s("querty")]).expect("execute");
    assert_java_eq(&s("oops"), &result);

    // succeeds with jxlt & strict navigation
    ctx.set("al", s("la")).expect("set");
    stmt = "(x)->{ x.`c${al}ss` ?? 'oops' }";
    sc = script(&engine, stmt);
    result = sc.execute_args(ctx.clone() as Arc<dyn JexlContext>, &[s("querty")]).expect("execute");
    assert_java_eq(&class_of("java.lang.String"), &result);

    // succeeds with jxlt & lenient navigation
    stmt = "(x)->{ x?.`c${al}ss` ?? 'oops' }";
    sc = script(&engine, stmt);
    result = sc.execute_args(ctx.clone() as Arc<dyn JexlContext>, &[s("querty")]).expect("execute");
    assert_java_eq(&class_of("java.lang.String"), &result);

    // fails with jxlt & lenient navigation
    stmt = "(x)->{ x?.`c${la}ss` ?? 'oops' }";
    sc = script(&engine, stmt);
    result = sc.execute_args(ctx.clone() as Arc<dyn JexlContext>, &[s("querty")]).expect("execute");
    assert_java_eq(&s("oops"), &result);

    // fails with jxlt & strict navigation
    stmt = "(x)->{ x.`c${la}ss` ?? 'oops' }";
    sc = script(&engine, stmt);
    result = sc.execute_args(ctx.clone() as Arc<dyn JexlContext>, &[s("querty")]).expect("execute");
    assert_java_eq(&s("oops"), &result);

    // fails with jxlt & lenient navigation
    stmt = "(x)->{ x?.`c${la--ss` ?? 'oops' }";
    sc = script(&engine, stmt);
    result = sc.execute_args(ctx.clone() as Arc<dyn JexlContext>, &[s("querty")]).expect("execute");
    assert_java_eq(&s("oops"), &result);

    // fails with jxlt & strict navigation
    stmt = "(x)->{ x.`c${la--ss` ?? 'oops' }";
    sc = script(&engine, stmt);
    result = sc.execute_args(ctx.clone() as Arc<dyn JexlContext>, &[s("querty")]).expect("execute");
    assert_java_eq(&s("oops"), &result);
}

// port of: PropertyAccessTest.test250
#[test]
fn test250() {
    let ctx = context();
    let x = hash_map(vec![(i(2), s("123456789"))]);
    ctx.set("x", x).expect("set");
    let engine = builder().strict(true).silent(false).create();
    let mut stmt = "x.2.class.name";
    let mut sc = script(&engine, stmt);
    let mut result = run(&sc, &ctx);
    assert_java_eq(&s("java.lang.String"), &result);

    stmt = "x.3?.class.name";
    sc = script(&engine, stmt);
    match sc.execute(ctx.clone() as Arc<dyn JexlContext>) {
        Ok(v) => assert!(v.is_null()),
        Err(_) => panic!("Should have evaluated to null"),
    }

    stmt = "x?.3.class.name";
    sc = script(&engine, stmt);
    match sc.execute(ctx.clone() as Arc<dyn JexlContext>) {
        Ok(_) => panic!("Should have thrown, fail on 3"),
        Err(e) => assert!(e.message().contains('3'), "detailedMessage: {}", e.message()),
    }

    stmt = "x?.3?.class.name";
    sc = script(&engine, stmt);
    match sc.execute(ctx.clone() as Arc<dyn JexlContext>) {
        Ok(v) => assert!(v.is_null()),
        Err(_) => panic!("Should have evaluated to null"),
    }

    stmt = "y?.3.class.name";
    sc = script(&engine, stmt);
    match sc.execute(ctx.clone() as Arc<dyn JexlContext>) {
        Ok(v) => assert!(v.is_null()),
        Err(_) => panic!("Should have evaluated to null"),
    }

    stmt = "x?.y?.z";
    sc = script(&engine, stmt);
    match sc.execute(ctx.clone() as Arc<dyn JexlContext>) {
        Ok(v) => assert!(v.is_null()),
        Err(_) => panic!("Should have evaluated to null"),
    }

    stmt = "x? (x.y? (x.y.z ?: null) :null) : null";
    sc = script(&engine, stmt);
    match sc.execute(ctx.clone() as Arc<dyn JexlContext>) {
        Ok(v) => assert!(v.is_null()),
        Err(_) => panic!("Should have evaluated to null"),
    }
    let _ = &mut result;
}

// port of: PropertyAccessTest.test275a
#[test]
fn test275a() {
    let jexl = builder().strict(true).safe(false).create();
    let ctxt = context();
    let p0 = Prompt::new();
    Prompt::set(&p0, "stuff", i(42));
    ctxt.set("$in", p0).expect("set");

    // unprotected navigation
    let mut sc = script_of(&jexl, "$in[p].intValue()", &["p"]);
    let mut result = Value::Null;
    match sc.execute_args(ctxt.clone() as Arc<dyn JexlContext>, &[s("fail")]) {
        Ok(_) => panic!("should have thrown a JexlException.Property"),
        Err(e) => assert_eq!("JexlException$Property", e.class_name()),
    }
    assert!(result.is_null());
    result = sc.execute_args(ctxt.clone() as Arc<dyn JexlContext>, &[s("stuff")]).expect("execute");
    assert_java_eq(&i(42), &result);

    // protected navigation
    sc = script_of(&jexl, "$in[p]?.intValue()", &["p"]);
    result = sc.execute_args(ctxt.clone() as Arc<dyn JexlContext>, &[s("fail")]).expect("execute");
    assert!(result.is_null());
    result = sc.execute_args(ctxt.clone() as Arc<dyn JexlContext>, &[s("stuff")]).expect("execute");
    assert_java_eq(&i(42), &result);

    // unprotected navigation
    sc = script_of(&jexl, "$in.`${p}`.intValue()", &["p"]);
    match sc.execute_args(ctxt.clone() as Arc<dyn JexlContext>, &[s("fail")]) {
        Ok(_) => panic!("should have thrown a JexlException.Property"),
        Err(e) => assert_eq!("JexlException$Property", e.class_name()),
    }
    result = sc.execute_args(ctxt.clone() as Arc<dyn JexlContext>, &[s("stuff")]).expect("execute");
    assert_java_eq(&i(42), &result);

    // protected navigation
    sc = script_of(&jexl, "$in.`${p}`?.intValue()", &["p"]);
    result = sc.execute_args(ctxt.clone() as Arc<dyn JexlContext>, &[s("fail")]).expect("execute");
    assert!(result.is_null());
    result = sc.execute_args(ctxt.clone() as Arc<dyn JexlContext>, &[s("stuff")]).expect("execute");
    assert_java_eq(&i(42), &result);
}

// port of: PropertyAccessTest.test275b
#[test]
fn test275b() {
    let jexl = builder().strict(true).safe(true).create();
    let ctxt = context();
    let p0 = Prompt::new();
    Prompt::set(&p0, "stuff", i(42));
    ctxt.set("$in", p0).expect("set");

    // unprotected navigation
    let mut sc = script_of(&jexl, "$in[p].intValue()", &["p"]);
    let mut result = sc.execute_args(ctxt.clone() as Arc<dyn JexlContext>, &[s("fail")]).expect("execute");
    assert!(result.is_null());

    result = sc.execute_args(ctxt.clone() as Arc<dyn JexlContext>, &[s("stuff")]).expect("execute");
    assert_java_eq(&i(42), &result);

    // unprotected navigation
    sc = script_of(&jexl, "$in.`${p}`.intValue()", &["p"]);
    result = sc.execute_args(ctxt.clone() as Arc<dyn JexlContext>, &[s("fail")]).expect("execute");
    assert!(result.is_null());
    result = sc.execute_args(ctxt.clone() as Arc<dyn JexlContext>, &[s("stuff")]).expect("execute");
    assert_java_eq(&i(42), &result);

    // protected navigation
    sc = script_of(&jexl, "$in.`${p}`?.intValue()", &["p"]);
    result = sc.execute_args(ctxt.clone() as Arc<dyn JexlContext>, &[s("fail")]).expect("execute");
    assert!(result.is_null());
    result = sc.execute_args(ctxt.clone() as Arc<dyn JexlContext>, &[s("stuff")]).expect("execute");
    assert_java_eq(&i(42), &result);
}

// =============================================================================== PublicFieldsTest

const LOWER42: &str = "fourty-two";
const UPPER42: &str = "FOURTY-TWO";

/// port of: PublicFieldsTest.setUp
fn public_fields_setup() -> (Arc<JexlEngine>, Value, Arc<MapContext>) {
    let jexl = jexl();
    let pub_ = Struct::new();
    let ctxt = context();
    ctxt.set("pub", pub_.clone()).expect("set");
    (jexl, pub_, ctxt)
}

// port of: PublicFieldsTest.testGetInt
#[test]
fn test_get_int() {
    let (jexl, pub_, ctxt) = public_fields_setup();
    let get = expression(&jexl, "pub.anInt");
    assert_java_eq(&i(42), &evaluate(&get, &ctxt));
    set_property(&jexl, &pub_, "anInt", &i(-42)).expect("setProperty");
    assert_java_eq(&i(-42), &evaluate(&get, &ctxt));
}

// port of: PublicFieldsTest.testSetInt
#[test]
fn test_set_int() {
    let (jexl, pub_, ctxt) = public_fields_setup();
    let set = expression(&jexl, "pub.anInt = value");
    ctxt.set("value", i(-42)).expect("set");
    assert_java_eq(&i(-42), &evaluate(&set, &ctxt));
    assert_java_eq(&i(-42), &get_property(&jexl, &pub_, "anInt").expect("getProperty"));
    ctxt.set("value", i(42)).expect("set");
    assert_java_eq(&i(42), &evaluate(&set, &ctxt));
    assert_java_eq(&i(42), &get_property(&jexl, &pub_, "anInt").expect("getProperty"));
    ctxt.set("value", s(UPPER42)).expect("set");
    match set.evaluate(ctxt.clone() as Arc<dyn JexlContext>) {
        Ok(v) => panic!("should have thrown, got {:?}", v.java_to_string()),
        Err(e) => assert!(e.is_jexl(), "not a JexlException: {}", e.class_name()),
    }
}

// port of: PublicFieldsTest.testGetString
#[test]
fn test_get_string() {
    let (jexl, pub_, ctxt) = public_fields_setup();
    let get = expression(&jexl, "pub.aString");
    assert_java_eq(&s(LOWER42), &evaluate(&get, &ctxt));
    set_property(&jexl, &pub_, "aString", &s(UPPER42)).expect("setProperty");
    assert_java_eq(&s(UPPER42), &evaluate(&get, &ctxt));
}

// port of: PublicFieldsTest.testSetString
#[test]
fn test_set_string() {
    let (jexl, pub_, ctxt) = public_fields_setup();
    let set = expression(&jexl, "pub.aString = value");
    ctxt.set("value", s(UPPER42)).expect("set");
    assert_java_eq(&s(UPPER42), &evaluate(&set, &ctxt));
    assert_java_eq(&s(UPPER42), &get_property(&jexl, &pub_, "aString").expect("getProperty"));
    ctxt.set("value", s(LOWER42)).expect("set");
    assert_java_eq(&s(LOWER42), &evaluate(&set, &ctxt));
    assert_java_eq(&s(LOWER42), &get_property(&jexl, &pub_, "aString").expect("getProperty"));
}

// port of: PublicFieldsTest.testGetInnerDouble
#[test]
fn test_get_inner_double() {
    let (jexl, pub_, ctxt) = public_fields_setup();
    let get = expression(&jexl, "pub.inner.aDouble");
    assert_java_eq(&d(42.0), &evaluate(&get, &ctxt));
    let inner = get_property(&jexl, &pub_, "inner").expect("getProperty");
    set_property(&jexl, &inner, "aDouble", &i(-42)).expect("setProperty");
    assert_java_eq(&d(-42.0), &evaluate(&get, &ctxt));
}

// port of: PublicFieldsTest.testSetInnerDouble
#[test]
fn test_set_inner_double() {
    let (jexl, pub_, ctxt) = public_fields_setup();
    let set = expression(&jexl, "pub.inner.aDouble = value");
    let inner = get_property(&jexl, &pub_, "inner").expect("getProperty");
    ctxt.set("value", d(-42.0)).expect("set");
    assert_java_eq(&d(-42.0), &evaluate(&set, &ctxt));
    assert_java_eq(&d(-42.0), &get_property(&jexl, &inner, "aDouble").expect("getProperty"));
    ctxt.set("value", d(42.0)).expect("set");
    assert_java_eq(&d(42.0), &evaluate(&set, &ctxt));
    assert_java_eq(&d(42.0), &get_property(&jexl, &inner, "aDouble").expect("getProperty"));
    ctxt.set("value", s(UPPER42)).expect("set");
    match set.evaluate(ctxt.clone() as Arc<dyn JexlContext>) {
        Ok(v) => panic!("should have thrown, got {:?}", v.java_to_string()),
        Err(e) => assert!(e.is_jexl(), "not a JexlException: {}", e.class_name()),
    }
}

