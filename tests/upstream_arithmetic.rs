//! The upstream Apache Commons JEXL 3.2.1 arithmetic tests, ported one method at a time.
//!
//! Sources: `ArithmeticTest`, `ArithmeticOperatorTest` and `BitwiseOperatorTest` of
//! `~/src/commons-jexl-3.2.1/src/test/java/org/apache/commons/jexl3/`.
//! Every expectation here is one the JVM produces: these three classes were built against
//! `commons-jexl3-3.2.1.jar` and run green on Corretto 25 before being transcribed.
#![allow(clippy::cloned_ref_to_slice_refs)] // ported Java test code
mod common;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use common::upstream::*;
use rust_jexl::jexl_arithmetic::JexlArithmetic;
use rust_jexl::jexl_context::JexlContext;
use rust_jexl::jexl_engine::{empty_context, JexlEngine, JexlScript};
use rust_jexl::value::Value;

// ----------------------------------------------------------------------------------- local sugar

fn script(jexl: &Arc<JexlEngine>, src: &str) -> JexlScript {
    jexl.create_script(src).expect("createScript")
}

fn script_of(jexl: &Arc<JexlEngine>, src: &str, params: &[&str]) -> JexlScript {
    let names: Vec<String> = params.iter().map(|p| (*p).to_string()).collect();
    jexl.create_script_named(src, &names).expect("createScript")
}

fn expression(jexl: &Arc<JexlEngine>, src: &str) -> JexlScript {
    jexl.create_expression(None, src).expect("createExpression")
}

/// `script.execute(null, args)`: Java's null context is `JexlEngine.EMPTY_CONTEXT`.
fn run_null(e: &JexlScript, args: &[Value]) -> Value {
    e.execute_args(empty_context(), args).expect("execute")
}

fn run_in(e: &JexlScript, ctx: &Arc<JexlEvalContext>, args: &[Value]) -> Value {
    e.execute_args(ctx.clone() as Arc<dyn JexlContext>, args).expect("execute")
}

fn byte_(n: i8) -> Value {
    Value::Byte(n)
}

fn short_(n: i16) -> Value {
    Value::Short(n)
}

fn float_(n: f32) -> Value {
    Value::Float(n)
}

fn var_value(v: &Value) -> i32 {
    Var::get(v)
}

// ================================================================================= ArithmeticTest

// port of: ArithmeticTest.testUndefinedVar
#[test]
fn test_undefined_var() {
    let asserter = Asserter::new(jexl());
    asserter.fail_expression("objects[1].status", Some(".*variable 'objects' is undefined.*"));
}

// port of: ArithmeticTest.testLeftNullOperand
#[test]
fn test_left_null_operand() {
    let asserter = Asserter::new(jexl());
    asserter.set_variable("left", Value::Null);
    asserter.set_variable("right", i(8));
    asserter.set_strict(true);
    for op in ["+", "-", "*", "/", "%", "&", "|", "^", "<", "<=", ">", ">="] {
        asserter.fail_expression(&format!("left {} right", op), Some(".*null.*"));
    }
}

// port of: ArithmeticTest.testLeftNullOperand2
#[test]
fn test_left_null_operand2() {
    let asserter = Asserter::new(jexl());
    asserter.set_variable("x.left", Value::Null);
    asserter.set_variable("right", i(8));
    asserter.set_strict(true);
    for op in ["+", "-", "*", "/", "%", "&", "|", "^", "<", "<=", ">", ">="] {
        asserter.fail_expression(&format!("x.left {} right", op), Some(".*null.*"));
    }
}

// port of: ArithmeticTest.testRightNullOperand
#[test]
fn test_right_null_operand() {
    let asserter = Asserter::new(jexl());
    asserter.set_variable("left", i(9));
    asserter.set_variable("right", Value::Null);
    for op in ["+", "-", "*", "/", "%", "&", "|", "^", "<", "<=", ">", ">="] {
        asserter.fail_expression(&format!("left {} right", op), Some(".*null.*"));
    }
}

// port of: ArithmeticTest.testRightNullOperand2
#[test]
fn test_right_null_operand2() {
    let asserter = Asserter::new(jexl());
    asserter.set_variable("left", i(9));
    asserter.set_variable("y.right", Value::Null);
    for op in ["+", "-", "*", "/", "%", "&", "|", "^", "<", "<=", ">", ">="] {
        asserter.fail_expression(&format!("left {} y.right", op), Some(".*null.*"));
    }
}

// port of: ArithmeticTest.testNullOperands
#[test]
fn test_null_operands() {
    let asserter = Asserter::new(jexl());
    asserter.set_variable("left", Value::Null);
    asserter.set_variable("right", Value::Null);
    for op in ["+", "-", "*", "/", "%", "&", "|", "^"] {
        asserter.fail_expression(&format!("left {} right", op), Some(".*null.*"));
    }
}

// port of: ArithmeticTest.testNullOperand
#[test]
fn test_null_operand() {
    let asserter = Asserter::new(jexl());
    asserter.set_variable("right", Value::Null);
    asserter.fail_expression("~right", Some(".*null.*"));
}

// port of: ArithmeticTest.testBigDecimal
#[test]
fn test_big_decimal() {
    let asserter = Asserter::new(jexl());
    asserter.set_variable("left", big_dec("2"));
    asserter.set_variable("right", big_dec("6"));
    asserter.assert_expression("left + right", &big_dec("8"));
    asserter.assert_expression("right - left", &big_dec("4"));
    asserter.assert_expression("right * left", &big_dec("12"));
    asserter.assert_expression("right / left", &big_dec("3"));
    asserter.assert_expression("right % left", &big_dec("0"));
}

// port of: ArithmeticTest.testBigInteger
#[test]
fn test_big_integer() {
    let asserter = Asserter::new(jexl());
    asserter.set_variable("left", big_int("2"));
    asserter.set_variable("right", big_int("6"));
    asserter.assert_expression("left + right", &big_int("8"));
    asserter.assert_expression("right - left", &big_int("4"));
    asserter.assert_expression("right * left", &big_int("12"));
    asserter.assert_expression("right / left", &big_int("3"));
    asserter.assert_expression("right % left", &big_int("0"));
}

// port of: ArithmeticTest.testOverflows
#[test]
fn test_overflows() {
    let asserter = Asserter::new(jexl());
    asserter.assert_expression("1 + 2147483647", &l(2147483648));
    asserter.assert_expression("3 + 9223372036854775805", &big_int("9223372036854775808"));
    asserter.assert_expression("-2147483648 - 1", &l(-2147483649));
    asserter.assert_expression("-3 + -9223372036854775806", &big_int("-9223372036854775809"));
    asserter.assert_expression("1 + 9223372036854775807", &big_int("9223372036854775808"));
    asserter.assert_expression("-1 + (-9223372036854775808)", &big_int("-9223372036854775809"));
    asserter.assert_expression("-9223372036854775808 - 1", &big_int("-9223372036854775809"));
    asserter.assert_expression(
        "9223372036854775807 * 9223372036854775807",
        &big_int("85070591730234615847396907784232501249"),
    );
}

// port of: ArithmeticTest.testUnaryMinus
#[test]
fn test_unary_minus() {
    let asserter = Asserter::new(jexl());
    asserter.set_variable("aByte", byte_(1));
    asserter.set_variable("aShort", short_(2));
    asserter.set_variable("anInteger", i(3));
    asserter.set_variable("aLong", l(4));
    asserter.set_variable("aFloat", float_(5.5));
    asserter.set_variable("aDouble", d(6.6));
    asserter.set_variable("aBigInteger", big_int("7"));
    asserter.set_variable("aBigDecimal", big_dec("8.8"));

    // loop to allow checking caching of constant numerals (debug)
    for _ in 0..2 {
        asserter.assert_expression("-3", &i(-3));
        asserter.assert_expression("-3.0", &d(-3.0));
        asserter.assert_expression("-aByte", &byte_(-1));
        asserter.assert_expression("-aShort", &short_(-2));
        asserter.assert_expression("-anInteger", &i(-3));
        asserter.assert_expression("-aLong", &l(-4));
        asserter.assert_expression("-aFloat", &float_(-5.5));
        asserter.assert_expression("-aDouble", &d(-6.6));
        asserter.assert_expression("-aBigInteger", &big_int("-7"));
        asserter.assert_expression("-aBigDecimal", &big_dec("-8.8"));
    }
}

// port of: ArithmeticTest.testUnaryPlus
#[test]
fn test_unary_plus() {
    let asserter = Asserter::new(jexl());
    asserter.set_variable("aByte", byte_(1));
    asserter.set_variable("aShort", short_(2));
    asserter.set_variable("anInteger", i(3));
    asserter.set_variable("aLong", l(4));
    asserter.set_variable("aFloat", float_(5.5));
    asserter.set_variable("aDouble", d(6.6));
    asserter.set_variable("aBigInteger", big_int("7"));
    asserter.set_variable("aBigDecimal", big_dec("8.8"));

    for _ in 0..2 {
        asserter.assert_expression("+3", &i(3));
        asserter.assert_expression("+3.0", &d(3.0));
        asserter.assert_expression("+aByte", &i(1));
        asserter.assert_expression("+aShort", &i(2));
        asserter.assert_expression("+anInteger", &i(3));
        asserter.assert_expression("+aLong", &l(4));
        asserter.assert_expression("+aFloat", &float_(5.5));
        asserter.assert_expression("+aDouble", &d(6.6));
        asserter.assert_expression("+aBigInteger", &big_int("7"));
        asserter.assert_expression("+aBigDecimal", &big_dec("8.8"));
    }
}

// port of: ArithmeticTest.testCalculations
#[test]
fn test_calculations() {
    let asserter = Asserter::new(jexl());
    asserter.set_strict2(true, false);
    // test new null coersion
    asserter.set_variable("imanull", Value::Null);
    asserter.assert_expression("imanull + 2", &i(2));
    asserter.assert_expression("imanull + imanull", &i(0));
    asserter.set_variable("foo", i(2));

    asserter.assert_expression("foo + 2", &i(4));
    asserter.assert_expression("3 + 3", &i(6));
    asserter.assert_expression("3 + 3 + foo", &i(8));
    asserter.assert_expression("3 * 3", &i(9));
    asserter.assert_expression("3 * 3 + foo", &i(11));
    asserter.assert_expression("3 * 3 - foo", &i(7));

    // test parenthesized exprs
    asserter.assert_expression("(4 + 3) * 6", &i(42));
    asserter.assert_expression("(8 - 2) * 7", &i(42));

    // test some floaty stuff
    asserter.assert_expression("3 * \"3.0\"", &d(9.0));
    asserter.assert_expression("3 * 3.0", &d(9.0));

    // test / and %
    asserter.set_strict2(false, false);
    asserter.assert_expression("6 / 3", &i(2));
    asserter.assert_expression("6.4 / 3", &d(6.4 / 3.0));
    asserter.assert_expression("0 / 3", &i(0));
    asserter.assert_expression("3 / 0", &d(0.0));
    asserter.assert_expression("4 % 3", &i(1));
    asserter.assert_expression("4.8 % 3", &d(4.8 % 3.0));
}

// port of: ArithmeticTest.testCoercions
#[test]
fn test_coercions() {
    let asserter = Asserter::new(jexl());
    asserter.assert_expression("1", &i(1)); // numerics default to Integer
    asserter.assert_expression("5L", &l(5));

    asserter.set_variable("I2", i(2));
    asserter.set_variable("L2", l(2));
    asserter.set_variable("L3", l(3));
    asserter.set_variable("B10", big_int("10"));

    // Integer & Integer => Integer
    asserter.assert_expression("I2 + 2", &i(4));
    asserter.assert_expression("I2 * 2", &i(4));
    asserter.assert_expression("I2 - 2", &i(0));
    asserter.assert_expression("I2 / 2", &i(1));

    // Integer & Long => Long
    asserter.assert_expression("I2 * L2", &l(4));
    asserter.assert_expression("I2 / L2", &l(1));

    // Long & Long => Long
    asserter.assert_expression("L2 + 3", &l(5));
    asserter.assert_expression("L2 + L3", &l(5));
    asserter.assert_expression("L2 / L2", &l(1));
    asserter.assert_expression("L2 / 2", &l(1));

    // BigInteger
    asserter.assert_expression("B10 / 10", &big_int("1"));
    asserter.assert_expression("B10 / I2", &big_int("5"));
    asserter.assert_expression("B10 / L2", &big_int("5"));
}

// port of: ArithmeticTest.testLongLiterals  (JEXL-24)
#[test]
fn test_long_literals() {
    let jexl = jexl();
    let ctxt = Arc::new(JexlEvalContext::new());
    ctxt.set_option(|o| o.set_strict_arithmetic(true));
    let stmt = "{a = 10L; b = 10l; c = 42.0D; d = 42.0d; e=56.3F; f=56.3f; g=63.5; h=0x10; i=010; j=0x10L; k=010l}";
    let expr = script(&jexl, stmt);
    run_in(&expr, &ctxt, &[]);
    assert_java_eq(&l(10), &ctxt.get("a").expect("a"));
    assert_java_eq(&l(10), &ctxt.get("b").expect("b"));
    assert_java_eq(&d(42.0), &ctxt.get("c").expect("c"));
    assert_java_eq(&d(42.0), &ctxt.get("d").expect("d"));
    assert_java_eq(&float_(56.3), &ctxt.get("e").expect("e"));
    assert_java_eq(&float_(56.3), &ctxt.get("f").expect("f"));
    assert_java_eq(&d(63.5), &ctxt.get("g").expect("g"));
    assert_java_eq(&i(0x10), &ctxt.get("h").expect("h"));
    assert_java_eq(&i(8), &ctxt.get("i").expect("i"));
    assert_java_eq(&l(0x10), &ctxt.get("j").expect("j"));
    assert_java_eq(&l(8), &ctxt.get("k").expect("k"));
}

// port of: ArithmeticTest.testBigLiteralValue
#[test]
fn test_big_literal_value() {
    let jexl = jexl();
    let ctxt = Arc::new(JexlEvalContext::new());
    ctxt.set_option(|o| o.set_strict_arithmetic(true));
    let e = expression(&jexl, "9223372036854775806.5B");
    let res = e.evaluate(ctxt.clone() as Arc<dyn JexlContext>).expect("evaluate").java_to_string();
    assert_eq!("9223372036854775806.5", res);
}

// port of: ArithmeticTest.testBigdOp
#[test]
fn test_bigd_op() {
    let jexl = jexl();
    let sevendot475 = big_dec("7.475");
    let so = big_dec("325");
    let jc = context();
    jc.set("SO", so).expect("set");

    let expr = "2.3*SO/100";

    let evaluated = expression(&jexl, expr).evaluate(jc as Arc<dyn JexlContext>).expect("evaluate");
    assert_java_eq(&sevendot475, &evaluated);
}

// port of: ArithmeticTest.testBigLiterals  (JEXL-24)
#[test]
fn test_big_literals() {
    let jexl = jexl();
    let ctxt = Arc::new(JexlEvalContext::new());
    ctxt.set_option(|o| o.set_strict_arithmetic(true));
    let stmt = "{a = 10H; b = 10h; c = 42.0B; d = 42.0b;}";
    run_in(&script(&jexl, stmt), &ctxt, &[]);
    assert_java_eq(&big_int("10"), &ctxt.get("a").expect("a"));
    assert_java_eq(&big_int("10"), &ctxt.get("b").expect("b"));
    assert_java_eq(&big_dec("42.0"), &ctxt.get("c").expect("c"));
    assert_java_eq(&big_dec("42.0"), &ctxt.get("d").expect("d"));
}

// port of: ArithmeticTest.testBigExponentLiterals  (JEXL-24)
#[test]
fn test_big_exponent_literals() {
    let jexl = jexl();
    let ctxt = Arc::new(JexlEvalContext::new());
    ctxt.set_option(|o| o.set_strict_arithmetic(true));
    let stmt = "{a = 42.0e1B; b = 42.0E+2B; c = 42.0e-1B; d = 42.0E-2b; e=4242.4242e1b}";
    run_in(&script(&jexl, stmt), &ctxt, &[]);
    assert_java_eq(&big_dec("42.0e+1"), &ctxt.get("a").expect("a"));
    assert_java_eq(&big_dec("42.0e+2"), &ctxt.get("b").expect("b"));
    assert_java_eq(&big_dec("42.0e-1"), &ctxt.get("c").expect("c"));
    assert_java_eq(&big_dec("42.0e-2"), &ctxt.get("d").expect("d"));
    assert_java_eq(&big_dec("4242.4242e1"), &ctxt.get("e").expect("e"));
}

// port of: ArithmeticTest.test2DoubleLiterals  (JEXL-24)
#[test]
fn test2_double_literals() {
    let jexl = jexl();
    let ctxt = Arc::new(JexlEvalContext::new());
    ctxt.set_option(|o| o.set_strict_arithmetic(true));
    let stmt = "{a = 42.0e1D; b = 42.0E+2D; c = 42.0e-1d; d = 42.0E-2d; e=10e10; f= +1.e1; g=1e1; }";
    run_in(&script(&jexl, stmt), &ctxt, &[]);
    assert_java_eq(&d(420.0), &ctxt.get("a").expect("a"));
    assert_java_eq(&d(4200.0), &ctxt.get("b").expect("b"));
    assert_java_eq(&d(4.2), &ctxt.get("c").expect("c"));
    assert_java_eq(&d(0.42), &ctxt.get("d").expect("d"));
    assert_java_eq(&d(10e10), &ctxt.get("e").expect("e"));
    assert_java_eq(&d(10.0), &ctxt.get("f").expect("f"));
    assert_java_eq(&d(10.0), &ctxt.get("g").expect("g"));
}

// port of: ArithmeticTest.testDivideByZero
//
// The trailing `debuggerCheck(jexl)` has no counterpart: the port does not expose the engine's
// expression cache, so there is nothing to walk back through the Debugger.
#[test]
fn test_divide_by_zero() {
    let context = Arc::new(JexlEvalContext::new());
    context.set_option(|o| o.set_strict_arithmetic(true));
    context.set("aByte", byte_(1)).expect("set");
    context.set("aShort", short_(2)).expect("set");
    context.set("aInteger", i(3)).expect("set");
    context.set("aLong", l(4)).expect("set");
    context.set("aFloat", float_(5.5)).expect("set");
    context.set("aDouble", d(6.6)).expect("set");
    context.set("aBigInteger", big_int("7")).expect("set");
    context.set("aBigDecimal", big_dec("8.8")).expect("set");

    context.set("zByte", byte_(0)).expect("set");
    context.set("zShort", short_(0)).expect("set");
    context.set("zInteger", i(0)).expect("set");
    context.set("zLong", l(0)).expect("set");
    context.set("zFloat", float_(0.0)).expect("set");
    context.set("zDouble", d(0.0)).expect("set");
    context.set("zBigInteger", big_int("0")).expect("set");
    context.set("zBigDecimal", big_dec("0")).expect("set");

    let tnames = ["Byte", "Short", "Integer", "Long", "Float", "Double", "BigInteger", "BigDecimal"];
    // number of permutations this will generate
    let perms = tnames.len() * tnames.len();

    let jexl = jexl();
    // for non-silent, silent...
    for s in 0..2 {
        let strict = s != 0;
        context.set_option(|o| {
            o.set_strict(true);
            o.set_strict_arithmetic(strict);
        });
        let mut zthrow = 0;
        let mut zeval = 0;
        // for vars of all types...
        for vname in tnames {
            // for zeros of all types...
            for zname in tnames {
                // divide var by zero
                let expr = format!("a{} / z{}", vname, zname);
                let outcome = jexl
                    .create_expression(None, &expr)
                    .and_then(|e| e.evaluate(context.clone() as Arc<dyn JexlContext>));
                match outcome {
                    Ok(nan) => {
                        // check we have a zero & increment zero count
                        if nan.is_number() {
                            let zero = jexl.get_arithmetic().to_double(&nan).unwrap_or(f64::NAN);
                            if zero == 0.0 {
                                zeval += 1;
                            }
                        }
                    }
                    // increment the exception count
                    Err(_) => zthrow += 1,
                }
            }
        }
        if strict {
            assert_eq!(zthrow, perms, "All expressions should have thrown {}/{}", zthrow, perms);
        } else {
            assert_eq!(zeval, perms, "All expressions should have zeroed {}/{}", zeval, perms);
        }
    }
}

// port of: ArithmeticTest.testMultClass  (JEXL-156)
#[test]
fn test_mult_class() {
    let jexl = create_engine();
    let jc = context();
    let ra = expression(&jexl, "463.0d * 0.1").evaluate(jc.clone() as Arc<dyn JexlContext>).expect("evaluate");
    assert_eq!("java.lang.Double", ra.class_name());
    let r0 = expression(&jexl, "463.0B * 0.1").evaluate(jc.clone() as Arc<dyn JexlContext>).expect("evaluate");
    assert_eq!("java.math.BigDecimal", r0.class_name());
    let r1 = expression(&jexl, "463.0B * 0.1B").evaluate(jc as Arc<dyn JexlContext>).expect("evaluate");
    assert_eq!("java.math.BigDecimal", r1.class_name());
}

// port of: ArithmeticTest.testDivClass
#[test]
fn test_div_class() {
    let jexl = create_engine();
    let jc = context();
    let ra = expression(&jexl, "463.0d / 0.1").evaluate(jc.clone() as Arc<dyn JexlContext>).expect("evaluate");
    assert_eq!("java.lang.Double", ra.class_name());
    let r0 = expression(&jexl, "463.0B / 0.1").evaluate(jc.clone() as Arc<dyn JexlContext>).expect("evaluate");
    assert_eq!("java.math.BigDecimal", r0.class_name());
    let r1 = expression(&jexl, "463.0B / 0.1B").evaluate(jc as Arc<dyn JexlContext>).expect("evaluate");
    assert_eq!("java.math.BigDecimal", r1.class_name());
}

// port of: ArithmeticTest.testPlusClass
#[test]
fn test_plus_class() {
    let jexl = create_engine();
    let jc = context();
    let ra = expression(&jexl, "463.0d + 0.1").evaluate(jc.clone() as Arc<dyn JexlContext>).expect("evaluate");
    assert_eq!("java.lang.Double", ra.class_name());
    let r0 = expression(&jexl, "463.0B + 0.1").evaluate(jc.clone() as Arc<dyn JexlContext>).expect("evaluate");
    assert_eq!("java.math.BigDecimal", r0.class_name());
    let r1 = expression(&jexl, "463.0B + 0.1B").evaluate(jc as Arc<dyn JexlContext>).expect("evaluate");
    assert_eq!("java.math.BigDecimal", r1.class_name());
}

// port of: ArithmeticTest.testMinusClass
#[test]
fn test_minus_class() {
    let jexl = create_engine();
    let jc = context();
    let ra = expression(&jexl, "463.0d - 0.1").evaluate(jc.clone() as Arc<dyn JexlContext>).expect("evaluate");
    assert_eq!("java.lang.Double", ra.class_name());
    let r0 = expression(&jexl, "463.0B - 0.1").evaluate(jc.clone() as Arc<dyn JexlContext>).expect("evaluate");
    assert_eq!("java.math.BigDecimal", r0.class_name());
    let r1 = expression(&jexl, "463.0B - 0.1B").evaluate(jc as Arc<dyn JexlContext>).expect("evaluate");
    assert_eq!("java.math.BigDecimal", r1.class_name());
}

// port of: ArithmeticTest.testAddWithStringsLenient
#[test]
fn test_add_with_strings_lenient() {
    let jexl = builder().arithmetic(JexlArithmetic::new(false, None, i32::MIN)).create();
    let mut result;

    result = run_null(&script(&jexl, "'a' + 0"), &[]);
    assert_java_eq(&s("a0"), &result);

    result = run_null(&script(&jexl, "0 + 'a' "), &[]);
    assert_java_eq(&s("0a"), &result);

    result = run_null(&script(&jexl, "0 + '1.2' "), &[]);
    assert_double_eq(1.2, &result, EPSILON);

    result = run_null(&script(&jexl, "'1.2' + 1.2 "), &[]);
    assert_double_eq(2.4, &result, EPSILON);

    result = run_null(&script(&jexl, "1.2 + 1.2 "), &[]);
    assert_double_eq(2.4, &result, EPSILON);

    result = run_null(&script(&jexl, "1.2 + '1.2' "), &[]);
    assert_double_eq(2.4, &result, EPSILON);

    result = run_null(&script(&jexl, "'1.2' + 0 "), &[]);
    assert_double_eq(1.2, &result, EPSILON);

    result = run_null(&script(&jexl, "'1.2' + '1.2' "), &[]);
    assert_java_eq(&s("1.21.2"), &result);
}

// port of: ArithmeticTest.testAddWithStringsStrict
#[test]
fn test_add_with_strings_strict() {
    let jexl = builder().arithmetic(JexlArithmetic::new(true, None, i32::MIN)).create();
    let mut result;

    result = run_null(&script(&jexl, "'a' + 0"), &[]);
    assert_java_eq(&s("a0"), &result);

    result = run_null(&script(&jexl, "0 + 'a' "), &[]);
    assert_java_eq(&s("0a"), &result);

    result = run_null(&script(&jexl, "0 + '1.2' "), &[]);
    assert_java_eq(&s("01.2"), &result);

    result = run_null(&script(&jexl, "'1.2' + 1.2 "), &[]);
    assert_java_eq(&s("1.21.2"), &result);

    result = run_null(&script(&jexl, "1.2 + 1.2 "), &[]);
    assert_double_eq(2.4, &result, EPSILON);

    result = run_null(&script(&jexl, "1.2 + '1.2' "), &[]);
    assert_java_eq(&s("1.21.2"), &result);

    result = run_null(&script(&jexl, "'1.2' + 0 "), &[]);
    assert_java_eq(&s("1.20"), &result);

    result = run_null(&script(&jexl, "'1.2' + '1.2' "), &[]);
    assert_java_eq(&s("1.21.2"), &result);
}

// port of: ArithmeticTest.testOption
#[test]
fn test_option() {
    let jexl = jexl();
    let context = Arc::new(JexlEvalContext::new());
    context.set_option(|o| o.set_strict_arithmetic(true));
    let sc = script(&jexl, "0 + '1.2' ");

    context.set_option(|o| o.set_strict_arithmetic(true));
    let mut result = run_in(&sc, &context, &[]);
    assert_java_eq(&s("01.2"), &result);

    context.set_option(|o| o.set_strict_arithmetic(false));
    result = run_in(&sc, &context, &[]);
    assert_double_eq(1.2, &result, EPSILON);
}

// port of: ArithmeticTest.testIsFloatingPointPattern
#[test]
fn test_is_floating_point_pattern() {
    let ja = JexlArithmetic::new(true, None, i32::MIN);

    for text in [
        "floating point", "a1.", "b1.2", "-10.2a-34", "+10.2a+34", "0", "1", "12A", "2F3", "23", "+3", "+34",
        "+3-4", "+3.-4", "3ee4",
    ] {
        assert!(!ja.is_floating_point_number(&s(text)), "isFloatingPointNumber({:?})", text);
    }

    for text in [
        "0.", "1.", "1.2", "1.2e3", "2e3", "+2e-3", "+23E-34", "+23.E-34", "-23.4E+45", "1.2e34", "10.2e34",
        "+10.2e34", "-10.2e34", "10.2e-34", "10.2e+34", "-10.2e-34", "+10.2e+34", "-10.2E-34", "+10.2E+34",
    ] {
        assert!(ja.is_floating_point_number(&s(text)), "isFloatingPointNumber({:?})", text);
    }
}

// port of: ArithmeticTest.testEmpty
#[test]
fn test_empty() {
    let scripts: [(&str, Value); 20] = [
        ("var x = null; log('x = %s', x);", i(0)),
        ("var x = 'abc'; log('x = %s', x);", i(1)),
        ("var x = 333; log('x = %s', x);", i(1)),
        ("var x = [1, 2]; log('x = %s', x);", i(2)),
        ("var x = ['a', 'b']; log('x = %s', x);", i(2)),
        ("var x = {1:'A', 2:'B'}; log('x = %s', x);", i(1)),
        ("var x = null; return empty(x);", Value::Boolean(true)),
        ("var x = ''; return empty(x);", Value::Boolean(true)),
        ("var x = 'abc'; return empty(x);", Value::Boolean(false)),
        ("var x = 0; return empty(x);", Value::Boolean(true)),
        ("var x = 333; return empty(x);", Value::Boolean(false)),
        ("var x = []; return empty(x);", Value::Boolean(true)),
        ("var x = [1, 2]; return empty(x);", Value::Boolean(false)),
        ("var x = ['a', 'b']; return empty(x);", Value::Boolean(false)),
        ("var x = [...]; return empty(x);", Value::Boolean(true)),
        ("var x = [1, 2,...]; return empty(x);", Value::Boolean(false)),
        ("var x = {:}; return empty(x);", Value::Boolean(true)),
        ("var x = {1:'A', 2:'B'}; return empty(x);", Value::Boolean(false)),
        ("var x = {}; return empty(x);", Value::Boolean(true)),
        ("var x = {'A','B'}; return empty(x);", Value::Boolean(false)),
    ];
    let jexl = create_engine();
    let jc = empty_test_context();

    for (stext, expected) in scripts {
        let sc = script(&jexl, stext);
        let result = run_in(&sc, &jc, &[]);
        assert!(expected.java_equals(&result), "failed on {}: got {:?}", stext, result.java_to_string());
    }
}

/// port of: ArithmeticTest.runOverload
fn run_overload(jexl: &Arc<JexlEngine>, jc: &Arc<JexlEvalContext>) {
    let mut sc;
    let mut result;

    sc = script(jexl, "(x, y)->{ x < y }");
    result = run_in(&sc, jc, &[i(42), i(43)]);
    assert_java_eq(&Value::Boolean(true), &result);
    result = run_in(&sc, jc, &[Var::new(42), Var::new(43)]);
    assert_java_eq(&Value::Boolean(true), &result);
    result = run_in(&sc, jc, &[Var::new(42), Var::new(43)]);
    assert_java_eq(&Value::Boolean(true), &result);
    result = run_in(&sc, jc, &[i(43), i(42)]);
    assert_java_eq(&Value::Boolean(false), &result);
    result = run_in(&sc, jc, &[Var::new(43), Var::new(42)]);
    assert_java_eq(&Value::Boolean(false), &result);

    sc = script(jexl, "(x, y)->{ x <= y }");
    result = run_in(&sc, jc, &[i(42), i(43)]);
    assert_java_eq(&Value::Boolean(true), &result);
    result = run_in(&sc, jc, &[Var::new(42), Var::new(43)]);
    assert_java_eq(&Value::Boolean(true), &result);
    result = run_in(&sc, jc, &[Var::new(41), Var::new(44)]);
    assert_java_eq(&Value::Boolean(true), &result);
    result = run_in(&sc, jc, &[i(43), i(42)]);
    assert_java_eq(&Value::Boolean(false), &result);
    result = run_in(&sc, jc, &[Var::new(45), Var::new(40)]);
    assert_java_eq(&Value::Boolean(false), &result);

    sc = script(jexl, "(x, y)->{ x > y }");
    result = run_in(&sc, jc, &[i(42), i(43)]);
    assert_java_eq(&Value::Boolean(false), &result);
    result = run_in(&sc, jc, &[Var::new(42), Var::new(43)]);
    assert_java_eq(&Value::Boolean(false), &result);
    result = run_in(&sc, jc, &[Var::new(42), Var::new(43)]);
    assert_java_eq(&Value::Boolean(false), &result);
    result = run_in(&sc, jc, &[i(43), i(42)]);
    assert_java_eq(&Value::Boolean(true), &result);
    result = run_in(&sc, jc, &[Var::new(43), Var::new(42)]);
    assert_java_eq(&Value::Boolean(true), &result);

    sc = script(jexl, "(x, y)->{ x >= y }");
    result = run_in(&sc, jc, &[i(42), i(43)]);
    assert_java_eq(&Value::Boolean(false), &result);
    result = run_in(&sc, jc, &[Var::new(42), Var::new(43)]);
    assert_java_eq(&Value::Boolean(false), &result);
    result = run_in(&sc, jc, &[Var::new(41), Var::new(44)]);
    assert_java_eq(&Value::Boolean(false), &result);
    result = run_in(&sc, jc, &[i(43), i(42)]);
    assert_java_eq(&Value::Boolean(true), &result);
    result = run_in(&sc, jc, &[Var::new(45), Var::new(40)]);
    assert_java_eq(&Value::Boolean(true), &result);

    sc = script(jexl, "(x, y)->{ x == y }");
    result = run_in(&sc, jc, &[i(42), i(43)]);
    assert_java_eq(&Value::Boolean(false), &result);
    result = run_in(&sc, jc, &[Var::new(42), Var::new(43)]);
    assert_java_eq(&Value::Boolean(false), &result);
    result = run_in(&sc, jc, &[Var::new(41), Var::new(44)]);
    assert_java_eq(&Value::Boolean(false), &result);
    result = run_in(&sc, jc, &[i(43), i(42)]);
    assert_java_eq(&Value::Boolean(false), &result);
    result = run_in(&sc, jc, &[Var::new(45), Var::new(40)]);
    assert_java_eq(&Value::Boolean(false), &result);

    sc = script(jexl, "(x, y)->{ x != y }");
    result = run_in(&sc, jc, &[i(42), i(43)]);
    assert_java_eq(&Value::Boolean(true), &result);
    result = run_in(&sc, jc, &[Var::new(42), Var::new(43)]);
    assert_java_eq(&Value::Boolean(true), &result);
    result = run_in(&sc, jc, &[Var::new(44), Var::new(44)]);
    assert_java_eq(&Value::Boolean(false), &result);
    result = run_in(&sc, jc, &[i(44), i(44)]);
    assert_java_eq(&Value::Boolean(false), &result);
    result = run_in(&sc, jc, &[Var::new(45), Var::new(40)]);
    assert_java_eq(&Value::Boolean(true), &result);

    sc = script(jexl, "(x, y)->{ x % y }");
    result = run_in(&sc, jc, &[i(4242), i(100)]);
    assert_java_eq(&i(42), &result);
    result = run_in(&sc, jc, &[Var::new(4242), Var::new(100)]);
    assert_eq!(42, var_value(&result));

    sc = script(jexl, "(x, y)->{ x * y }");
    result = run_in(&sc, jc, &[i(6), i(7)]);
    assert_java_eq(&i(42), &result);
    result = run_in(&sc, jc, &[Var::new(6), Var::new(7)]);
    assert_eq!(42, var_value(&result));

    sc = script(jexl, "(x, y)->{ x + y }");
    result = run_in(&sc, jc, &[i(35), i(7)]);
    assert_java_eq(&i(42), &result);
    result = run_in(&sc, jc, &[Var::new(35), Var::new(7)]);
    assert_eq!(42, var_value(&result));

    sc = script(jexl, "(x, y)->{ x - y }");
    result = run_in(&sc, jc, &[i(49), i(7)]);
    assert_java_eq(&i(42), &result);
    result = run_in(&sc, jc, &[s("foobarquux"), s("bar")]);
    assert_java_eq(&s("fooquux"), &result);
    result = run_in(&sc, jc, &[i(50), i(8)]);
    assert_java_eq(&i(42), &result);
    result = run_in(&sc, jc, &[Var::new(50), Var::new(8)]);
    assert_eq!(42, var_value(&result));

    sc = script(jexl, "(x)->{ -x }");
    result = run_in(&sc, jc, &[i(-42)]);
    assert_java_eq(&i(42), &result);
    result = run_in(&sc, jc, &[Var::new(-42)]);
    assert_eq!(42, var_value(&result));
    result = run_in(&sc, jc, &[s("pizza")]);
    assert_java_eq(&s("azzip"), &result);
    result = run_in(&sc, jc, &[i(-142)]);
    assert_java_eq(&i(142), &result);

    sc = script(jexl, "(x)->{ ~x }");
    result = run_in(&sc, jc, &[i(-1)]);
    assert_java_eq(&l(0), &result);
    result = run_in(&sc, jc, &[Var::new(-1)]);
    assert_eq!(0, var_value(&result));
    result = run_in(&sc, jc, &[Var::new(-42)]);
    assert_eq!(41, var_value(&result));

    sc = script(jexl, "(x, y)->{ x ^ y }");
    result = run_in(&sc, jc, &[i(35), i(7)]);
    assert_java_eq(&l(36), &result);
    result = run_in(&sc, jc, &[Var::new(35), Var::new(7)]);
    assert_eq!(36, var_value(&result));

    sc = script(jexl, "(x, y)->{ x & y }");
    result = run_in(&sc, jc, &[i(35), i(7)]);
    assert_java_eq(&l(3), &result);
    result = run_in(&sc, jc, &[Var::new(35), Var::new(7)]);
    assert_eq!(3, var_value(&result));

    sc = script(jexl, "(x, y)->{ x =^ y }");
    result = run_in(&sc, jc, &[i(3115), i(31)]);
    assert_java_eq(&Value::Boolean(false), &result);
    result = run_in(&sc, jc, &[Var::new(3115), Var::new(31)]);
    assert_java_eq(&Value::Boolean(true), &result);

    sc = script(jexl, "(x, y)->{ x !^ y }");
    result = run_in(&sc, jc, &[i(3115), i(31)]);
    assert_java_eq(&Value::Boolean(true), &result);
    result = run_in(&sc, jc, &[Var::new(3115), Var::new(31)]);
    assert_java_eq(&Value::Boolean(false), &result);

    sc = script(jexl, "(x, y)->{ x =$ y }");
    result = run_in(&sc, jc, &[i(3115), i(15)]);
    assert_java_eq(&Value::Boolean(false), &result);
    result = run_in(&sc, jc, &[Var::new(3115), Var::new(15)]);
    assert_java_eq(&Value::Boolean(true), &result);

    sc = script(jexl, "(x, y)->{ x !$ y }");
    result = run_in(&sc, jc, &[i(3115), i(15)]);
    assert_java_eq(&Value::Boolean(true), &result);
    result = run_in(&sc, jc, &[Var::new(3115), Var::new(15)]);
    assert_java_eq(&Value::Boolean(false), &result);

    sc = script(jexl, "(x, y)->{ x =~ y }");
    result = run_in(&sc, jc, &[i(3155), i(15)]);
    assert_java_eq(&Value::Boolean(false), &result);
    result = run_in(&sc, jc, &[Var::new(3155), Var::new(15)]);
    assert_java_eq(&Value::Boolean(false), &result);
    result = run_in(&sc, jc, &[Var::new(15), Var::new(3155)]);
    assert_java_eq(&Value::Boolean(true), &result);

    sc = script(jexl, "(x, y)->{ x !~ y }");
    result = run_in(&sc, jc, &[i(3115), i(15)]);
    assert_java_eq(&Value::Boolean(true), &result);
    result = run_in(&sc, jc, &[Var::new(3155), Var::new(15)]);
    assert_java_eq(&Value::Boolean(true), &result);
    result = run_in(&sc, jc, &[Var::new(15), Var::new(3155)]);
    assert_java_eq(&Value::Boolean(false), &result);

    sc = script(jexl, "(x)->{ !x }");
    match sc.execute_args(jc.clone() as Arc<dyn JexlContext>, &[Var::new(-42)]) {
        Ok(_) => panic!("should fail"),
        Err(xany) => assert_eq!("JexlException$Operator", xany.class_name()),
    }
}

// port of: ArithmeticTest.testArithmeticPlus
#[test]
fn test_arithmetic_plus() {
    let jexl = arithmetic_plus_engine(64);
    let jc = empty_test_context();
    run_overload(&jexl, &jc);
    run_overload(&jexl, &jc);
}

// port of: ArithmeticTest.testArithmeticPlusNoCache
#[test]
fn test_arithmetic_plus_no_cache() {
    let jexl = arithmetic_plus_engine(0);
    let jc = empty_test_context();
    run_overload(&jexl, &jc);
}

// port of: ArithmeticTest.testJexl173
#[test]
fn test_jexl173() {
    let jexl = create_engine();
    let jc = context();
    let c173 = Callable173::new();
    let mut e = script_of(&jexl, "c173(9, 6)", &["c173"]);
    let mut result = e.execute_args(jc.clone() as Arc<dyn JexlContext>, &[c173.clone()]).expect("execute");
    assert_java_eq(&i(54), &result);
    e = script_of(&jexl, "c173('fourty', 'two')", &["c173"]);
    result = e.execute_args(jc as Arc<dyn JexlContext>, &[c173]).expect("execute");
    assert_java_eq(&i(42), &result);
}

// port of: ArithmeticTest.testEmptyLong
#[test]
fn test_empty_long() {
    let jexl = jexl();
    let mut x;
    x = run_null(&script(&jexl, "new('java.lang.Long', 4294967296)"), &[]);
    assert_eq!(4294967296i64, match x {
        Value::Long(v) => v,
        _ => panic!("not a Long"),
    });
    x = run_null(&script(&jexl, "new('java.lang.Long', '4294967296')"), &[]);
    assert_java_eq(&l(4294967296), &x);
    x = run_null(&script(&jexl, "4294967296l"), &[]);
    assert_java_eq(&l(4294967296), &x);
    x = run_null(&script(&jexl, "4294967296L"), &[]);
    assert_java_eq(&l(4294967296), &x);
    check_empty(&jexl, &x, false);
    x = run_null(&script(&jexl, "0L"), &[]);
    assert_java_eq(&l(0), &x);
    check_empty(&jexl, &x, true);
}

// port of: ArithmeticTest.testEmptyFloat
#[test]
fn test_empty_float() {
    let jexl = jexl();
    let mut x;
    x = run_null(&script(&jexl, "4294967296.f"), &[]);
    assert_double_eq(4294967296.0, &x, EPSILON);
    check_empty(&jexl, &x, false);
    x = run_null(&script(&jexl, "4294967296.0f"), &[]);
    assert_double_eq(4294967296.0, &x, EPSILON);
    check_empty(&jexl, &x, false);
    x = run_null(&script(&jexl, "0.0f"), &[]);
    assert_double_eq(0.0, &x, EPSILON);
    check_empty(&jexl, &x, true);
    x = float_(f32::NAN);
    check_empty(&jexl, &x, true);
}

// port of: ArithmeticTest.testEmptyDouble
#[test]
fn test_empty_double() {
    let jexl = jexl();
    let mut x;
    x = run_null(&script(&jexl, "4294967296.d"), &[]);
    assert_double_eq(4294967296.0, &x, EPSILON);
    check_empty(&jexl, &x, false);
    x = run_null(&script(&jexl, "4294967296.0d"), &[]);
    assert_double_eq(4294967296.0, &x, EPSILON);
    check_empty(&jexl, &x, false);
    x = run_null(&script(&jexl, "0.0d"), &[]);
    assert_double_eq(0.0, &x, EPSILON);
    check_empty(&jexl, &x, true);
    x = d(f64::NAN);
    check_empty(&jexl, &x, true);
}

/// port of: ArithmeticTest.checkEmpty
fn check_empty(jexl: &Arc<JexlEngine>, x: &Value, expect: bool) {
    let s0 = script_of(jexl, "empty(x)", &["x"]);
    let mut empty = run_null(&s0, std::slice::from_ref(x));
    assert_java_eq(&Value::Boolean(expect), &empty);
    let s1 = script_of(jexl, "empty x", &["x"]);
    empty = run_null(&s1, std::slice::from_ref(x));
    assert_java_eq(&Value::Boolean(expect), &empty);
    let s2 = script_of(jexl, "x.empty()", &["x"]);
    empty = run_null(&s2, std::slice::from_ref(x));
    assert_java_eq(&Value::Boolean(expect), &empty);
}

// port of: ArithmeticTest.testCoerceInteger
#[test]
fn test_coerce_integer() {
    let jexl = jexl();
    let ja = jexl.get_arithmetic();
    let ctxt = Arc::new(JexlEvalContext::new());
    ctxt.set_option(|o| o.set_strict_arithmetic(true));
    let stmt = "a = 34L; b = 45.0D; c=56.0F; d=67B; e=78H;";
    run_in(&script(&jexl, stmt), &ctxt, &[]);
    assert_eq!(34, ja.to_integer(&ctxt.get("a").expect("a")).expect("toInteger"));
    assert_eq!(45, ja.to_integer(&ctxt.get("b").expect("b")).expect("toInteger"));
    assert_eq!(56, ja.to_integer(&ctxt.get("c").expect("c")).expect("toInteger"));
    assert_eq!(67, ja.to_integer(&ctxt.get("d").expect("d")).expect("toInteger"));
    assert_eq!(78, ja.to_integer(&ctxt.get("e").expect("e")).expect("toInteger"));
    assert_eq!(10, ja.to_integer(&s("10")).expect("toInteger"));
    assert_eq!(1, ja.to_integer(&Value::Boolean(true)).expect("toInteger"));
    assert_eq!(0, ja.to_integer(&Value::Boolean(false)).expect("toInteger"));
}

// port of: ArithmeticTest.testCoerceLong
#[test]
fn test_coerce_long() {
    let jexl = jexl();
    let ja = jexl.get_arithmetic();
    let ctxt = Arc::new(JexlEvalContext::new());
    ctxt.set_option(|o| o.set_strict_arithmetic(true));
    let stmt = "a = 34L; b = 45.0D; c=56.0F; d=67B; e=78H;";
    run_in(&script(&jexl, stmt), &ctxt, &[]);
    assert_eq!(34i64, ja.to_long(&ctxt.get("a").expect("a")).expect("toLong"));
    assert_eq!(45i64, ja.to_long(&ctxt.get("b").expect("b")).expect("toLong"));
    assert_eq!(56i64, ja.to_long(&ctxt.get("c").expect("c")).expect("toLong"));
    assert_eq!(67i64, ja.to_long(&ctxt.get("d").expect("d")).expect("toLong"));
    assert_eq!(78i64, ja.to_long(&ctxt.get("e").expect("e")).expect("toLong"));
    assert_eq!(10i64, ja.to_long(&s("10")).expect("toLong"));
    assert_eq!(1i64, ja.to_long(&Value::Boolean(true)).expect("toLong"));
    assert_eq!(0i64, ja.to_long(&Value::Boolean(false)).expect("toLong"));
}

// port of: ArithmeticTest.testCoerceDouble
#[test]
fn test_coerce_double() {
    let jexl = jexl();
    let ja = jexl.get_arithmetic();
    let ctxt = Arc::new(JexlEvalContext::new());
    ctxt.set_option(|o| o.set_strict_arithmetic(true));
    let stmt = "{a = 34L; b = 45.0D; c=56.0F; d=67B; e=78H; }";
    run_in(&script(&jexl, stmt), &ctxt, &[]);
    for (name, want) in [("a", 34.0), ("b", 45.0), ("c", 56.0), ("d", 67.0), ("e", 78.0)] {
        let got = ja.to_double(&ctxt.get(name).expect("var")).expect("toDouble");
        assert!((want - got).abs() <= EPSILON, "{}: {} != {}", name, want, got);
    }
    assert!((10.0 - ja.to_double(&s("10")).expect("toDouble")).abs() <= EPSILON);
    assert!((1.0 - ja.to_double(&Value::Boolean(true)).expect("toDouble")).abs() <= EPSILON);
    assert!((0.0 - ja.to_double(&Value::Boolean(false)).expect("toDouble")).abs() <= EPSILON);
}

// port of: ArithmeticTest.testCoerceBigInteger
#[test]
fn test_coerce_big_integer() {
    let jexl = jexl();
    let ja = jexl.get_arithmetic();
    let ctxt = Arc::new(JexlEvalContext::new());
    ctxt.set_option(|o| o.set_strict_arithmetic(true));
    let stmt = "{a = 34L; b = 45.0D; c=56.0F; d=67B; e=78H; }";
    run_in(&script(&jexl, stmt), &ctxt, &[]);
    for (name, want) in [("a", "34"), ("b", "45"), ("c", "56"), ("d", "67"), ("e", "78")] {
        let got = Value::big_integer(ja.to_big_integer(&ctxt.get(name).expect("var")).expect("toBigInteger"));
        assert_java_eq(&big_int(want), &got);
    }
    assert_java_eq(&big_int("10"), &Value::big_integer(ja.to_big_integer(&s("10")).expect("toBigInteger")));
    assert_java_eq(
        &big_int("1"),
        &Value::big_integer(ja.to_big_integer(&Value::Boolean(true)).expect("toBigInteger")),
    );
    assert_java_eq(
        &big_int("0"),
        &Value::big_integer(ja.to_big_integer(&Value::Boolean(false)).expect("toBigInteger")),
    );
}

// port of: ArithmeticTest.testCoerceBigDecimal
#[test]
fn test_coerce_big_decimal() {
    let jexl = jexl();
    let ja = jexl.get_arithmetic();
    let ctxt = Arc::new(JexlEvalContext::new());
    ctxt.set_option(|o| o.set_strict_arithmetic(true));
    let stmt = "{a = 34L; b = 45.0D; c=56.0F; d=67B; e=78H; }";
    run_in(&script(&jexl, stmt), &ctxt, &[]);
    // BigDecimal.valueOf(long) has scale 0; BigDecimal.valueOf(double) goes through Double.toString
    for (name, want) in [("a", "34"), ("b", "45.0"), ("c", "56.0"), ("d", "67"), ("e", "78")] {
        let got = Value::big_decimal(ja.to_big_decimal(&ctxt.get(name).expect("var")).expect("toBigDecimal"));
        assert_java_eq(&big_dec(want), &got);
    }
    assert_java_eq(&big_dec("10"), &Value::big_decimal(ja.to_big_decimal(&s("10")).expect("toBigDecimal")));
    assert_java_eq(
        &big_dec("1.0"),
        &Value::big_decimal(ja.to_big_decimal(&Value::Boolean(true)).expect("toBigDecimal")),
    );
    assert_java_eq(
        &big_dec("0.0"),
        &Value::big_decimal(ja.to_big_decimal(&Value::Boolean(false)).expect("toBigDecimal")),
    );
}

// port of: ArithmeticTest.testAtomicBoolean
#[test]
fn test_atomic_boolean() {
    let jexl = jexl();
    // in a condition
    let mut e = script_of(&jexl, "if (x) 1 else 2;", &["x"]);
    let jc = context();
    let flag = Arc::new(AtomicBool::new(false));
    let ab = Value::AtomicBoolean(flag.clone());
    let mut o;
    o = e.execute_args(jc.clone() as Arc<dyn JexlContext>, &[ab.clone()]).expect("execute");
    assert_java_eq(&i(2), &o);
    flag.store(true, Ordering::Relaxed);
    o = e.execute_args(jc.clone() as Arc<dyn JexlContext>, &[ab.clone()]).expect("execute");
    assert_java_eq(&i(1), &o);
    // in a binary logical op
    e = script_of(&jexl, "x && y", &["x", "y"]);
    flag.store(true, Ordering::Relaxed);
    o = e.execute_args(jc.clone() as Arc<dyn JexlContext>, &[ab.clone(), Value::Boolean(false)]).expect("execute");
    assert_java_eq(&Value::Boolean(false), &o);
    flag.store(true, Ordering::Relaxed);
    o = e.execute_args(jc.clone() as Arc<dyn JexlContext>, &[ab.clone(), Value::Boolean(true)]).expect("execute");
    assert_java_eq(&Value::Boolean(true), &o);
    flag.store(false, Ordering::Relaxed);
    o = e.execute_args(jc.clone() as Arc<dyn JexlContext>, &[ab.clone(), Value::Boolean(false)]).expect("execute");
    assert_java_eq(&Value::Boolean(false), &o);
    flag.store(false, Ordering::Relaxed);
    o = e.execute_args(jc.clone() as Arc<dyn JexlContext>, &[ab.clone(), Value::Boolean(false)]).expect("execute");
    assert_java_eq(&Value::Boolean(false), &o);
    // in arithmetic op
    e = script_of(&jexl, "x + y", &["x", "y"]);
    flag.store(true, Ordering::Relaxed);
    o = e.execute_args(jc.clone() as Arc<dyn JexlContext>, &[ab.clone(), i(10)]).expect("execute");
    assert_java_eq(&i(11), &o);
    o = e.execute_args(jc.clone() as Arc<dyn JexlContext>, &[i(10), ab.clone()]).expect("execute");
    assert_java_eq(&i(11), &o);
    o = e.execute_args(jc.clone() as Arc<dyn JexlContext>, &[ab.clone(), d(10.0)]).expect("execute");
    assert_double_eq(11.0, &o, EPSILON);
    o = e.execute_args(jc.clone() as Arc<dyn JexlContext>, &[d(10.0), ab.clone()]).expect("execute");
    assert_double_eq(11.0, &o, EPSILON);

    let bi10 = big_int("10");
    flag.store(false, Ordering::Relaxed);
    o = e.execute_args(jc.clone() as Arc<dyn JexlContext>, &[ab.clone(), bi10.clone()]).expect("execute");
    assert_java_eq(&bi10, &o);
    o = e.execute_args(jc.clone() as Arc<dyn JexlContext>, &[bi10.clone(), ab.clone()]).expect("execute");
    assert_java_eq(&bi10, &o);

    let bd10 = big_dec("10");
    flag.store(false, Ordering::Relaxed);
    o = e.execute_args(jc.clone() as Arc<dyn JexlContext>, &[ab.clone(), bd10.clone()]).expect("execute");
    assert_java_eq(&bd10, &o);
    o = e.execute_args(jc.clone() as Arc<dyn JexlContext>, &[bd10.clone(), ab.clone()]).expect("execute");
    assert_java_eq(&bd10, &o);

    // in a (the) monadic op
    e = script_of(&jexl, "!x", &["x"]);
    flag.store(true, Ordering::Relaxed);
    o = e.execute_args(jc.clone() as Arc<dyn JexlContext>, &[ab.clone()]).expect("execute");
    assert_java_eq(&Value::Boolean(false), &o);
    flag.store(false, Ordering::Relaxed);
    o = e.execute_args(jc.clone() as Arc<dyn JexlContext>, &[ab.clone()]).expect("execute");
    assert_java_eq(&Value::Boolean(true), &o);

    // in a (the) monadic op
    e = script_of(&jexl, "-x", &["x"]);
    flag.store(true, Ordering::Relaxed);
    o = e.execute_args(jc.clone() as Arc<dyn JexlContext>, &[ab.clone()]).expect("execute");
    assert_java_eq(&Value::Boolean(false), &o);
    flag.store(false, Ordering::Relaxed);
    o = e.execute_args(jc as Arc<dyn JexlContext>, &[ab]).expect("execute");
    assert_java_eq(&Value::Boolean(true), &o);
}

// ========================================================================= ArithmeticOperatorTest

/// port of: ArithmeticOperatorTest.setUp
fn operator_asserter() -> Asserter {
    let asserter = Asserter::new(jexl());
    asserter.set_strict(false);
    asserter
}

fn string_builder(text: &str) -> Value {
    // the port has no public StringBuilder constructor; `new` is the JEXL-level one Java uses
    let jexl = jexl();
    jexl.create_script(&format!("new('java.lang.StringBuilder', '{}')", text))
        .expect("createScript")
        .execute(empty_context())
        .expect("execute")
}

fn pattern(regex: &str) -> Value {
    Value::object(rust_jexl::value::PatternValue(Arc::new(
        rust_jexl::java::regex::Pattern::compile(regex).expect("compile"),
    )))
}

// port of: ArithmeticOperatorTest.testRegexp
#[test]
fn test_regexp() {
    let asserter = operator_asserter();
    asserter.set_variable("str", s("abc456"));
    asserter.assert_expression("str =~ '.*456'", &Value::Boolean(true));
    asserter.assert_expression("str !~ 'ABC.*'", &Value::Boolean(true));
    asserter.set_variable("match", s("abc.*"));
    asserter.set_variable("nomatch", s(".*123"));
    asserter.assert_expression("str =~ match", &Value::Boolean(true));
    asserter.assert_expression("str !~ match", &Value::Boolean(false));
    asserter.assert_expression("str !~ nomatch", &Value::Boolean(true));
    asserter.assert_expression("str =~ nomatch", &Value::Boolean(false));
    asserter.set_variable("match", string_builder("abc.*"));
    asserter.set_variable("nomatch", string_builder(".*123"));
    asserter.assert_expression("str =~ match", &Value::Boolean(true));
    asserter.assert_expression("str !~ match", &Value::Boolean(false));
    asserter.assert_expression("str !~ nomatch", &Value::Boolean(true));
    asserter.assert_expression("str =~ nomatch", &Value::Boolean(false));
    asserter.set_variable("match", pattern("abc.*"));
    asserter.set_variable("nomatch", pattern(".*123"));
    asserter.assert_expression("str =~ match", &Value::Boolean(true));
    asserter.assert_expression("str !~ match", &Value::Boolean(false));
    asserter.assert_expression("str !~ nomatch", &Value::Boolean(true));
    asserter.assert_expression("str =~ nomatch", &Value::Boolean(false));
    // check the in/not-in variant
    asserter.assert_expression("'a' =~ ['a','b','c','d','e','f']", &Value::Boolean(true));
    asserter.assert_expression("'a' !~ ['a','b','c','d','e','f']", &Value::Boolean(false));
    asserter.assert_expression("'z' =~ ['a','b','c','d','e','f']", &Value::Boolean(false));
    asserter.assert_expression("'z' !~ ['a','b','c','d','e','f']", &Value::Boolean(true));
}

// port of: ArithmeticOperatorTest.testRegexp2
#[test]
fn test_regexp2() {
    let asserter = operator_asserter();
    asserter.set_variable("str", s("abc456"));
    asserter.assert_expression("str =~ ~/.*456/", &Value::Boolean(true));
    asserter.assert_expression("str !~ ~/ABC.*/", &Value::Boolean(true));
    asserter.assert_expression("str =~ ~/abc\\d{3}/", &Value::Boolean(true));
    asserter.set_variable("str", s("4/6"));
    asserter.assert_expression("str =~ ~/\\d\\/\\d/", &Value::Boolean(true));
}

// port of: ArithmeticOperatorTest.testStartsEndsWithString
#[test]
fn test_starts_ends_with_string() {
    let asserter = operator_asserter();
    asserter.set_variable("x", s("foobar"));
    asserter.assert_expression("x =^ 'foo'", &Value::Boolean(true));
    asserter.assert_expression("x =$ 'foo'", &Value::Boolean(false));
    asserter.set_variable("x", s("barfoo"));
    asserter.assert_expression("x =^ 'foo'", &Value::Boolean(false));
    asserter.assert_expression("x =$ 'foo'", &Value::Boolean(true));
}

// port of: ArithmeticOperatorTest.testStartsEndsWithStringDot
#[test]
fn test_starts_ends_with_string_dot() {
    let asserter = operator_asserter();
    asserter.set_variable("x.y", s("foobar"));
    asserter.assert_expression("x.y =^ 'foo'", &Value::Boolean(true));
    asserter.assert_expression("x.y =$ 'foo'", &Value::Boolean(false));
    asserter.set_variable("x.y", s("barfoo"));
    asserter.assert_expression("x.y =^ 'foo'", &Value::Boolean(false));
    asserter.assert_expression("x.y =$ 'foo'", &Value::Boolean(true));
}

// port of: ArithmeticOperatorTest.testNotStartsEndsWithString
#[test]
fn test_not_starts_ends_with_string() {
    let asserter = operator_asserter();
    asserter.set_variable("x", s("foobar"));
    asserter.assert_expression("x !^ 'foo'", &Value::Boolean(false));
    asserter.assert_expression("x !$ 'foo'", &Value::Boolean(true));
    asserter.set_variable("x", s("barfoo"));
    asserter.assert_expression("x !^ 'foo'", &Value::Boolean(true));
    asserter.assert_expression("x !$ 'foo'", &Value::Boolean(false));
}

// port of: ArithmeticOperatorTest.testNotStartsEndsWithStringDot
#[test]
fn test_not_starts_ends_with_string_dot() {
    let asserter = operator_asserter();
    asserter.set_variable("x.y", s("foobar"));
    asserter.assert_expression("x.y !^ 'foo'", &Value::Boolean(false));
    asserter.assert_expression("x.y !$ 'foo'", &Value::Boolean(true));
    asserter.set_variable("x.y", s("barfoo"));
    asserter.assert_expression("x.y !^ 'foo'", &Value::Boolean(true));
    asserter.assert_expression("x.y !$ 'foo'", &Value::Boolean(false));
}

// port of: ArithmeticOperatorTest.testStartsEndsWithStringBuilder
#[test]
fn test_starts_ends_with_string_builder() {
    let asserter = operator_asserter();
    asserter.set_variable("x", string_builder("foobar"));
    asserter.assert_expression("x =^ 'foo'", &Value::Boolean(true));
    asserter.assert_expression("x =$ 'foo'", &Value::Boolean(false));
    asserter.set_variable("x", string_builder("barfoo"));
    asserter.assert_expression("x =^ 'foo'", &Value::Boolean(false));
    asserter.assert_expression("x =$ 'foo'", &Value::Boolean(true));
}

// port of: ArithmeticOperatorTest.testNotStartsEndsWithStringBuilder
#[test]
fn test_not_starts_ends_with_string_builder() {
    let asserter = operator_asserter();
    asserter.set_variable("x", string_builder("foobar"));
    asserter.assert_expression("x !^ 'foo'", &Value::Boolean(false));
    asserter.assert_expression("x !$ 'foo'", &Value::Boolean(true));
    asserter.set_variable("x", string_builder("barfoo"));
    asserter.assert_expression("x !^ 'foo'", &Value::Boolean(true));
    asserter.assert_expression("x !$ 'foo'", &Value::Boolean(false));
}

// port of: ArithmeticOperatorTest.testMatch
#[test]
fn test_match() {
    // check in/not-in on array, list, map, set and duck-type collection
    let ai = [2, 4, 42, 54];
    let al = list(ai.iter().map(|n| i(*n)).collect());
    let am = hash_map(vec![
        (i(2), s("two")),
        (i(4), s("four")),
        (i(42), s("forty-two")),
        (i(54), s("fifty-four")),
    ]);
    let ad = MatchingContainer::new(&ai);
    let ic = IterableContainer::new(&ai);
    let as_ = hash_set(ai.iter().map(|n| i(*n)).collect());
    let vars = [int_array(&ai), al, am, ad, as_, ic];

    let asserter = operator_asserter();
    for var in vars {
        asserter.set_variable("container", var);
        for x in ai {
            asserter.set_variable("x", i(x));
            asserter.assert_expression("x =~ container", &Value::Boolean(true));
        }
        asserter.set_variable("x", i(169));
        asserter.assert_expression("x !~ container", &Value::Boolean(true));
    }
}

// port of: ArithmeticOperatorTest.testStartsEndsWith
#[test]
fn test_starts_ends_with() {
    let asserter = operator_asserter();
    asserter.set_variable("x", s("foobar"));
    asserter.assert_expression("x =^ 'foo'", &Value::Boolean(true));
    asserter.assert_expression("x =$ 'foo'", &Value::Boolean(false));
    asserter.set_variable("x", s("barfoo"));
    asserter.assert_expression("x =^ 'foo'", &Value::Boolean(false));
    asserter.assert_expression("x =$ 'foo'", &Value::Boolean(true));

    let ai = [2, 4, 42, 54];
    let ic = IterableContainer::new(&ai);
    asserter.set_variable("x", ic);
    asserter.assert_expression("x =^ 2", &Value::Boolean(true));
    asserter.assert_expression("x =$ 54", &Value::Boolean(true));
    asserter.assert_expression("x =^ 4", &Value::Boolean(false));
    asserter.assert_expression("x =$ 42", &Value::Boolean(false));
    asserter.assert_expression("x =^ [2, 4]", &Value::Boolean(true));
    asserter.assert_expression("x =^ [42, 54]", &Value::Boolean(true));
}

// port of: ArithmeticOperatorTest.testNotStartsEndsWith
#[test]
fn test_not_starts_ends_with() {
    let asserter = operator_asserter();
    asserter.set_variable("x", s("foobar"));
    asserter.assert_expression("x !^ 'foo'", &Value::Boolean(false));
    asserter.assert_expression("x !$ 'foo'", &Value::Boolean(true));
    asserter.set_variable("x", s("barfoo"));
    asserter.assert_expression("x !^ 'foo'", &Value::Boolean(true));
    asserter.assert_expression("x !$ 'foo'", &Value::Boolean(false));

    let ai = [2, 4, 42, 54];
    let ic = IterableContainer::new(&ai);
    asserter.set_variable("x", ic);
    asserter.assert_expression("x !^ 2", &Value::Boolean(false));
    asserter.assert_expression("x !$ 54", &Value::Boolean(false));
    asserter.assert_expression("x !^ 4", &Value::Boolean(true));
    asserter.assert_expression("x !$ 42", &Value::Boolean(true));
    asserter.assert_expression("x !^ [2, 4]", &Value::Boolean(false));
    asserter.assert_expression("x !^ [42, 54]", &Value::Boolean(false));
}

// port of: ArithmeticOperatorTest.testInterval
#[test]
fn test_interval() {
    let ns = namespaces(&[("calc", Aggregate::new())]);
    let jexl = builder().namespaces(ns).create();
    let mut sc;
    let mut result;

    sc = script(&jexl, "1 .. 3");
    result = run_null(&sc, &[]);
    let values = iterate(&result);
    assert_java_eq(&i(1), &values[0]);
    assert_java_eq(&i(2), &values[1]);
    assert_java_eq(&i(3), &values[2]);

    sc = script(&jexl, "(4 - 3) .. (9 / 3)");
    result = run_null(&sc, &[]);
    let values = iterate(&result);
    assert_java_eq(&i(1), &values[0]);
    assert_java_eq(&i(2), &values[1]);
    assert_java_eq(&i(3), &values[2]);

    // sum of 1, 2, 3
    sc = script(&jexl, "var x = 0; for(var y : ((5 - 4) .. (12 / 4))) { x = x + y }; x");
    result = run_null(&sc, &[]);
    assert_java_eq(&i(6), &result);

    sc = script(&jexl, "calc:sum(1 .. 3)");
    result = run_null(&sc, &[]);
    assert_java_eq(&i(6), &result);

    sc = script(&jexl, "calc:sum(-3 .. 3)");
    result = run_null(&sc, &[]);
    assert_java_eq(&i(0), &result);
}

// ============================================================================ BitwiseOperatorTest

/// port of: BitwiseOperatorTest.setUp
fn bitwise_asserter() -> Asserter {
    let asserter = Asserter::new(jexl());
    asserter.set_strict2(false, false);
    asserter
}

// port of: BitwiseOperatorTest.testAndWithTwoNulls
#[test]
fn test_and_with_two_nulls() {
    bitwise_asserter().assert_expression("null & null", &l(0));
}

// port of: BitwiseOperatorTest.testAndWithLeftNull
#[test]
fn test_and_with_left_null() {
    bitwise_asserter().assert_expression("null & 1", &l(0));
}

// port of: BitwiseOperatorTest.testAndWithRightNull
#[test]
fn test_and_with_right_null() {
    bitwise_asserter().assert_expression("1 & null", &l(0));
}

// port of: BitwiseOperatorTest.testAndSimple
#[test]
fn test_and_simple() {
    bitwise_asserter().assert_expression("15 & 3", &l(15 & 3));
}

// port of: BitwiseOperatorTest.testAndVariableNumberCoercion
#[test]
fn test_and_variable_number_coercion() {
    let asserter = bitwise_asserter();
    asserter.set_variable("x", i(15));
    asserter.set_variable("y", short_(7));
    asserter.assert_expression("x & y", &l(15 & 7));
}

// port of: BitwiseOperatorTest.testAndVariableStringCoercion
#[test]
fn test_and_variable_string_coercion() {
    let asserter = bitwise_asserter();
    asserter.set_variable("x", i(15));
    asserter.set_variable("y", s("7"));
    asserter.assert_expression("x & y", &l(15 & 7));
}

// port of: BitwiseOperatorTest.testComplementWithNull
#[test]
fn test_complement_with_null() {
    bitwise_asserter().assert_expression("~null", &l(-1));
}

// port of: BitwiseOperatorTest.testComplementSimple
#[test]
fn test_complement_simple() {
    bitwise_asserter().assert_expression("~128", &l(-129));
}

// port of: BitwiseOperatorTest.testComplementVariableNumberCoercion
#[test]
fn test_complement_variable_number_coercion() {
    let asserter = bitwise_asserter();
    asserter.set_variable("x", i(15));
    asserter.assert_expression("~x", &l(!15));
}

// port of: BitwiseOperatorTest.testComplementVariableStringCoercion
#[test]
fn test_complement_variable_string_coercion() {
    let asserter = bitwise_asserter();
    asserter.set_variable("x", s("15"));
    asserter.assert_expression("~x", &l(!15));
}

// port of: BitwiseOperatorTest.testOrWithTwoNulls
#[test]
fn test_or_with_two_nulls() {
    bitwise_asserter().assert_expression("null | null", &l(0));
}

// port of: BitwiseOperatorTest.testOrWithLeftNull
#[test]
fn test_or_with_left_null() {
    bitwise_asserter().assert_expression("null | 1", &l(1));
}

// port of: BitwiseOperatorTest.testOrWithRightNull
#[test]
fn test_or_with_right_null() {
    bitwise_asserter().assert_expression("1 | null", &l(1));
}

// port of: BitwiseOperatorTest.testOrSimple
#[test]
fn test_or_simple() {
    bitwise_asserter().assert_expression("12 | 3", &l(15));
}

// port of: BitwiseOperatorTest.testOrVariableNumberCoercion
#[test]
fn test_or_variable_number_coercion() {
    let asserter = bitwise_asserter();
    asserter.set_variable("x", i(12));
    asserter.set_variable("y", short_(3));
    asserter.assert_expression("x | y", &l(15));
}

// port of: BitwiseOperatorTest.testOrVariableStringCoercion
#[test]
fn test_or_variable_string_coercion() {
    let asserter = bitwise_asserter();
    asserter.set_variable("x", i(12));
    asserter.set_variable("y", s("3"));
    asserter.assert_expression("x | y", &l(15));
}

// port of: BitwiseOperatorTest.testXorWithTwoNulls
#[test]
fn test_xor_with_two_nulls() {
    bitwise_asserter().assert_expression("null ^ null", &l(0));
}

// port of: BitwiseOperatorTest.testXorWithLeftNull
#[test]
fn test_xor_with_left_null() {
    bitwise_asserter().assert_expression("null ^ 1", &l(1));
}

// port of: BitwiseOperatorTest.testXorWithRightNull
#[test]
fn test_xor_with_right_null() {
    bitwise_asserter().assert_expression("1 ^ null", &l(1));
}

// port of: BitwiseOperatorTest.testXorSimple
#[test]
fn test_xor_simple() {
    bitwise_asserter().assert_expression("1 ^ 3", &l(1 ^ 3));
}

// port of: BitwiseOperatorTest.testXorVariableNumberCoercion
#[test]
fn test_xor_variable_number_coercion() {
    let asserter = bitwise_asserter();
    asserter.set_variable("x", i(1));
    asserter.set_variable("y", short_(3));
    asserter.assert_expression("x ^ y", &l(1 ^ 3));
}

// port of: BitwiseOperatorTest.testXorVariableStringCoercion
#[test]
fn test_xor_variable_string_coercion() {
    let asserter = bitwise_asserter();
    asserter.set_variable("x", i(1));
    asserter.set_variable("y", s("3"));
    asserter.assert_expression("x ^ y", &l(1 ^ 3));
}

// port of: BitwiseOperatorTest.testParenthesized
#[test]
fn test_parenthesized() {
    let asserter = bitwise_asserter();
    asserter.assert_expression("(2 | 1) & 3", &l(3));
    asserter.assert_expression("(2 & 1) | 3", &l(3));
    asserter.assert_expression("~(120 | 42)", &l(!(120 | 42)));
}
