//! Ports of the upstream lexical-scope, local-variable, lambda, annotation and pragma test
//! classes of Apache Commons JEXL 3.2.1 (`src/test/java/org/apache/commons/jexl3/`).
//!
//! Every `#[test]` is one Java test method, named in snake_case, with the Java class and method
//! in a comment above it. Expected values were produced by running the same script, engine
//! configuration and context through the real `commons-jexl3-3.2.1.jar` (`oracle/target/oracle`).
//!
//! `JexlTestCase` installs `JexlOptions.setDefaultFlags("-safe", "+lexical")` for the whole
//! upstream suite; `builder()` below is that default. `LexicalTest.testOptionsPragma` is the one
//! test that runs under the library defaults instead, and says so.
//! The upstream test methods of these classes that are not here are listed, with their
//! reason, in COMPATIBILITY.md.
#![allow(clippy::bool_assert_comparison)]

use std::any::Any;
use std::collections::BTreeSet;
use std::sync::{Arc, Mutex, RwLock};

use rust_jexl::internal::lexical_scope::LexicalScope;
use rust_jexl::internal::template_interpreter::StringWriter;
use rust_jexl::introspection::jdk_shim::{HostIntrospector, JdkShim};
use rust_jexl::introspection::uberspect::Uberspect;
use rust_jexl::introspection::{JexlMethod, JexlPropertyGet, JexlPropertySet, ResolverStrategy};
use rust_jexl::jexl_context::{JexlContext, MapContext};
use rust_jexl::jexl_engine::{empty_context, JexlBuilder, JexlEngine, JexlScript};
use rust_jexl::jexl_exception::JexlException;
use rust_jexl::jexl_features::JexlFeatures;
use rust_jexl::jexl_options::JexlOptions;
use rust_jexl::value::{Component, HostObject, JArray, JList, Value};

// ------------------------------------------------------------------ harness

/// port of: JexlTestCase's static initializer, `JexlOptions.setDefaultFlags("-safe", "+lexical")`.
fn builder() -> JexlBuilder {
    hosts(JexlBuilder::new().safe(false).lexical(true))
}

/// port of: `new JexlBuilder().cache(128).create()`, the JEXL field of every JexlTestCase.
fn jexl() -> Arc<JexlEngine> {
    builder().cache(128).create()
}

/// port of: JexlTestCase.createEngine(), `new JexlBuilder().create()`.
fn create_engine() -> Arc<JexlEngine> {
    builder().create()
}

fn hosts(b: JexlBuilder) -> JexlBuilder {
    let shim = JdkShim::new(ResolverStrategy::Jexl).with_hosts(Arc::new(Beans));
    b.uberspect(Arc::new(Uberspect::new().with_shim(Arc::new(shim))))
}

/// The options a `new JexlOptions()` carries under the upstream suite's default flags.
fn test_options() -> JexlOptions {
    let mut o = JexlOptions::new();
    o.set_safe(false);
    o.set_lexical(true);
    o
}

fn ok(r: Result<Value, JexlException>) -> Value {
    r.unwrap_or_else(|e| panic!("unexpected exception: {}", e.message()))
}

fn thrown(r: Result<Value, JexlException>) -> JexlException {
    match r {
        Ok(v) => panic!("should have thrown, got {:?}", v),
        Err(e) => e,
    }
}

fn parse_err(r: Result<JexlScript, JexlException>) -> JexlException {
    match r {
        Ok(_) => panic!("should not have been parsed"),
        Err(e) => e,
    }
}

#[track_caller]
fn eq(got: &Value, want: &Value) {
    assert!(got.java_equals(want), "expected {:?}, got {:?}", want, got);
}

fn i(n: i32) -> Value {
    Value::Integer(n)
}
fn s(t: &str) -> Value {
    Value::string(t)
}
fn list(items: Vec<Value>) -> Value {
    Value::List(JList::array_list(items))
}
fn vars_of(script: &JexlScript) -> BTreeSet<Vec<String>> {
    script
        .get_variables()
        .iter()
        .map(|v| v.iter().map(|p| p.to_rust()).collect())
        .collect()
}
fn refs(paths: &[&[&str]]) -> BTreeSet<Vec<String>> {
    paths.iter().map(|p| p.iter().map(|x| x.to_string()).collect()).collect()
}

// ------------------------------------------------------------------ contexts

/// port of: org.apache.commons.jexl3.JexlEvalContext — variables plus mutable engine options.
/// Also stands for the upstream subclasses that only add a namespace (`NumbersContext`,
/// `EnhancedContext`) or a pragma/annotation hook.
struct EvalContext {
    vars: MapContext,
    options: RwLock<JexlOptions>,
    null_namespace: Option<Value>,
    /// AnnotationTest.AnnotationContext: how many annotations were processed, and their names
    annotations: Mutex<(i32, BTreeSet<String>)>,
    /// LexicalTest.VarContext: pragmas are applied to `options`
    pragma_processor: bool,
    annotation_processor: bool,
}

impl EvalContext {
    fn build(options: JexlOptions) -> EvalContext {
        EvalContext {
            vars: MapContext::new(),
            options: RwLock::new(options),
            null_namespace: None,
            annotations: Mutex::new((0, BTreeSet::new())),
            pragma_processor: false,
            annotation_processor: false,
        }
    }

    fn new() -> Arc<EvalContext> {
        Arc::new(EvalContext::build(test_options()))
    }

    /// port of: VarTest.NumbersContext — the null namespace resolves to an object with methods.
    fn with_null_namespace(ns: Value) -> Arc<EvalContext> {
        let mut c = EvalContext::build(test_options());
        c.null_namespace = Some(ns);
        Arc::new(c)
    }

    /// port of: AnnotationTest.AnnotationContext
    fn annotating() -> Arc<EvalContext> {
        let mut c = EvalContext::build(test_options());
        c.annotation_processor = true;
        Arc::new(c)
    }

    /// port of: LexicalTest.VarContext (its options are a plain `new JexlOptions()`)
    fn var_context() -> Arc<EvalContext> {
        let mut c = EvalContext::build(JexlOptions::new());
        c.pragma_processor = true;
        Arc::new(c)
    }

    fn options(&self, f: impl FnOnce(&mut JexlOptions)) {
        f(&mut self.options.write().unwrap())
    }

    /// port of: LexicalTest.VarContext.snatchOptions
    fn snatch_options(&self) -> JexlOptions {
        let mut o = self.options.write().unwrap();
        std::mem::replace(&mut *o, JexlOptions::new())
    }

    fn count(&self) -> i32 {
        self.annotations.lock().unwrap().0
    }

    fn names(&self) -> BTreeSet<String> {
        self.annotations.lock().unwrap().1.clone()
    }
}

impl JexlContext for EvalContext {
    fn get(&self, name: &str) -> Option<Value> {
        self.vars.get(name)
    }
    fn set(&self, name: &str, value: Value) -> Result<(), String> {
        self.vars.set(name, value)
    }
    fn has(&self, name: &str) -> bool {
        self.vars.has(name)
    }
    fn get_engine_options(&self) -> Option<JexlOptions> {
        Some(self.options.read().unwrap().clone())
    }
    fn resolve_namespace(&self, name: Option<&str>) -> Option<Value> {
        match name {
            Some(_) => None,
            None => self.null_namespace.clone(),
        }
    }
    fn is_namespace_resolver(&self) -> bool {
        self.null_namespace.is_some()
    }
    fn is_pragma_processor(&self) -> bool {
        self.pragma_processor
    }
    // port of: LexicalTest.VarContext.processPragma
    fn process_pragma(&self, key: &rust_jexl::java::string::JString, value: &Value) {
        if key.to_rust() == "jexl.options" && value.java_to_string() == "canonical" {
            let mut o = self.options.write().unwrap();
            o.set_strict(true);
            o.set_lexical(true);
            o.set_lexical_shade(true);
            o.set_safe(false);
        }
    }
    // port of: AnnotationTest.AnnotationContext.processAnnotation (the counting part), and
    // LexicalTest.OptAnnotationContext's transient @scale side effect.
    fn process_annotation(
        &self,
        name: &str,
        args: Option<&[Value]>,
        statement: &mut dyn FnMut() -> Result<Value, JexlException>,
    ) -> Option<Result<Value, JexlException>> {
        if !self.annotation_processor {
            return None;
        }
        {
            let mut a = self.annotations.lock().unwrap();
            a.0 += 1;
            a.1.insert(name.to_string());
            match (name, args) {
                ("one", Some(v)) if !v.is_empty() => {
                    a.1.insert(v[0].java_to_string());
                }
                ("two", Some(v)) if v.len() > 1 => {
                    a.1.insert(v[0].java_to_string());
                    a.1.insert(v[1].java_to_string());
                }
                ("error", Some(v)) if !v.is_empty() => {
                    let a0 = v[0].java_to_string();
                    a.1.insert(a0.clone());
                    return Some(Err(JexlException::java(
                        "java.lang.IllegalArgumentException",
                        Some(a0),
                    )));
                }
                // returns without running the statement: Java makes that an annotation error
                ("unknown", _) => return Some(Ok(Value::Null)),
                ("scale", Some(v)) if !v.is_empty() => {
                    if let Value::Integer(n) = v[0] {
                        self.options.write().unwrap().set_math_scale(n);
                    }
                }
                _ => {}
            }
        }
        Some(statement())
    }
}

/// port of: org.apache.commons.jexl3.ReadonlyContext
struct ReadonlyContext(Arc<EvalContext>);

impl JexlContext for ReadonlyContext {
    fn get(&self, name: &str) -> Option<Value> {
        self.0.get(name)
    }
    fn set(&self, _name: &str, _value: Value) -> Result<(), String> {
        Err("Not supported in readonly context.".into())
    }
    fn has(&self, name: &str) -> bool {
        self.0.has(name)
    }
    fn get_engine_options(&self) -> Option<JexlOptions> {
        self.0.get_engine_options()
    }
}

// ------------------------------------------------------------------ host objects

/// port of: the object `VarTest.NumbersContext` answers with for the null namespace (Java returns
/// the context itself, which carries `numbers()`).
#[derive(Debug)]
struct Numbers;

impl HostObject for Numbers {
    fn class_name(&self) -> String {
        "org.apache.commons.jexl3.VarTest$NumbersContext".into()
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

struct HostMethod {
    ret: &'static str,
    call: fn(&Value, &[Value]) -> Result<Value, JexlException>,
}

impl JexlMethod for HostMethod {
    fn invoke(&self, obj: &Value, params: &[Value]) -> Result<Value, JexlException> {
        (self.call)(obj, params)
    }
    fn return_type(&self) -> Option<String> {
        Some(self.ret.to_string())
    }
    fn is_cacheable(&self) -> bool {
        true
    }
}

struct Beans;

impl HostIntrospector for Beans {
    fn get_method(&self, obj: &Value, name: &str, args: &[Value]) -> Option<Arc<dyn JexlMethod>> {
        if obj.as_host::<Numbers>().is_some() && name == "numbers" && args.is_empty() {
            return Some(Arc::new(HostMethod {
                ret: "java.lang.Object",
                call: |_, _| Ok(Value::Array(JArray::new(Component::Int, vec![i(5), i(17), i(20)]))),
            }));
        }
        None
    }

    fn get_property_get(&self, _obj: &Value, _identifier: &Value) -> Option<Arc<dyn JexlPropertyGet>> {
        None
    }

    fn get_property_set(&self, _obj: &Value, _identifier: &Value, _arg: &Value) -> Option<Arc<dyn JexlPropertySet>> {
        None
    }

    fn get_constructor(&self, _handle: &Value, _args: &[Value]) -> Option<Arc<dyn JexlMethod>> {
        None
    }
}

// ================================================================== LexicalTest

/// port of: LexicalTest.runLexical0 — each script must fail, at parse time when the lexical
/// feature is on and at run time otherwise.
fn lexical0_fails(
    jexl: &Arc<JexlEngine>,
    ctxt: Arc<dyn JexlContext>,
    src: &str,
    params: Option<&[String]>,
    feature: bool,
    args: &[Value],
) {
    let script = match jexl.create_script_info(None, src, params) {
        Err(_) => return,
        Ok(script) => script,
    };
    if !feature {
        thrown(script.execute_args(ctxt, args));
    } else {
        panic!("should have failed: {}", src);
    }
}

fn run_lexical0(feature: bool) {
    let f = JexlFeatures::new().lexical(feature);
    let jexl = builder().strict(true).features(f).create();
    let ctxt = EvalContext::new();
    // ensure errors will throw
    ctxt.options(|o| o.set_lexical(true));
    let none: Option<&[String]> = None;
    lexical0_fails(&jexl, ctxt.clone(), "var x = 0; var x = 1;", none, feature, &[]);
    lexical0_fails(&jexl, ctxt.clone(), "var x = 0; for(var y : null) { var y = 1;", none, feature, &[]);
    lexical0_fails(&jexl, ctxt.clone(), "var x = 0; for(var x : null) {};", none, feature, &[]);
    lexical0_fails(&jexl, ctxt.clone(), "(x)->{ var x = 0; x; }", none, feature, &[]);
    lexical0_fails(&jexl, ctxt.clone(), "var x; if (true) { if (true) { var x = 0; x; } }", none, feature, &[]);
    lexical0_fails(
        &jexl,
        ctxt.clone(),
        "if (a) { var y = (x)->{ var x = 0; x; }; y(2) }",
        Some(&["a".to_string()]),
        feature,
        &[],
    );
    lexical0_fails(&jexl, ctxt.clone(), "(x)->{ for(var x : null) { x; } }", none, feature, &[i(42)]);
    // no fail
    let script = jexl
        .create_script("var x = 32; (()->{ for(var x : null) { x; }})();")
        .expect("parse");
    if !feature {
        eq(&ok(script.execute_args(ctxt, &[i(42)])), &Value::Null);
    }
}

// port of: LexicalTest.testLexical0a
#[test]
fn test_lexical0a() {
    run_lexical0(false);
}

// port of: LexicalTest.testLexical0b
#[test]
fn test_lexical0b() {
    run_lexical0(true);
}

fn run_lexical1(shade: bool) {
    let jexl = builder().strict(true).create();
    let ctxt = EvalContext::new();
    ctxt.set("x", i(4242)).expect("set");
    // ensure errors will throw
    ctxt.options(|o| {
        o.set_lexical(true);
        o.set_lexical_shade(shade);
    });

    for src in ["{ var x = 0; } x", "{ var x = 0; } x = 42", "{ var x = 0; } y = 42"] {
        let script = jexl.create_script(src).expect("parse");
        match script.execute(ctxt.clone()) {
            Ok(_) => assert!(!shade, "local shade means '{}' should be undefined", src),
            Err(e) => assert!(shade, "{}", e.message()),
        }
    }
    // no fail
    let script = jexl
        .create_script("var x = 32; (()->{ for(var x : null) { x; }})();")
        .expect("parse");
    eq(&ok(script.execute_args(ctxt.clone(), &[i(42)])), &Value::Null);

    // y being defined as global
    ctxt.set("y", i(4242)).expect("set");
    let script = jexl.create_script("{ var y = 0; } y = 42").expect("parse");
    match script.execute(ctxt) {
        Ok(v) => {
            assert!(!shade, "local shade means 'y = 42' should be undefined");
            eq(&v, &i(42));
        }
        Err(e) => assert!(shade, "{}", e.message()),
    }
}

// port of: LexicalTest.testLexical1a
#[test]
fn test_lexical1a() {
    run_lexical1(false);
}

// port of: LexicalTest.testLexical1b
#[test]
fn test_lexical1b() {
    run_lexical1(true);
}

// port of: LexicalTest.testLexical1
#[test]
fn test_lexical1() {
    let jexl = builder().strict(true).create();
    let ctxt = EvalContext::new();
    ctxt.options(|o| o.set_lexical(true));

    let script = jexl
        .create_script("var x = 0; for(var y : [1]) { var x = 42; return x; };")
        .expect("parse");
    thrown(script.execute(ctxt.clone()));

    let script = jexl.create_script("(x)->{ if (x) { var x = 7 * (x + x); x; } }").expect("parse");
    thrown(script.execute_args(ctxt.clone(), &[i(3)]));

    let script = jexl.create_script("{ var x = 0; } var x = 42; x").expect("parse");
    eq(&ok(script.execute_args(ctxt, &[i(21)])), &i(42));
}

fn run_lexical2(lexical: bool) {
    let jexl = builder().strict(true).lexical(lexical).create();
    let ctxt: Arc<dyn JexlContext> = Arc::new(MapContext::new());
    let script = jexl.create_script("{var x = 42}; {var x; return x; }").expect("parse");
    let result = ok(script.execute(ctxt));
    if lexical {
        eq(&result, &Value::Null);
    } else {
        eq(&result, &i(42));
    }
}

// port of: LexicalTest.testLexical2a
#[test]
fn test_lexical2a() {
    run_lexical2(true);
}

// port of: LexicalTest.testLexical2b
#[test]
fn test_lexical2b() {
    run_lexical2(false);
}

// port of: LexicalTest.testLexical3
#[test]
fn test_lexical3() {
    let str_ = "var s = {}; for (var i : [1]) s.add(i); s";
    let jexl = builder().strict(true).lexical(true).create();
    let jc: Arc<dyn JexlContext> = Arc::new(MapContext::new());

    let e = jexl.create_script(str_).expect("parse");
    let o = ok(e.execute(jc.clone()));
    assert!(matches!(&o, Value::Set(s) if s.contains(&i(1))), "{:?}", o);

    let e = jexl.create_script(str_).expect("parse");
    let o = ok(e.execute(jc));
    assert!(matches!(&o, Value::Set(s) if s.contains(&i(1))), "{:?}", o);
}

// port of: LexicalTest.testLexical4
#[test]
fn test_lexical4() {
    let jexl_ = builder().silent(false).strict(true).lexical(true).create();
    let jxlt = rust_jexl::jxlt_engine::create_jxlt_engine(&jexl_);
    let ctxt: Arc<dyn JexlContext> = Arc::new(MapContext::new());
    let rpt = "<report>\n\n$$var y = 1; var x = 2;\n${x + y}\n</report>\n";
    let t = jxlt.create_template_str(rpt, None).expect("template");
    let strw = StringWriter::new();
    t.evaluate(ctxt, Some(strw.clone()), &[]).expect("evaluate");
    let output = strw.to_jstring().to_rust();
    assert_eq!(output, "<report>\n\n3\n</report>\n");
}

// port of: LexicalTest.testLexical5
// adapted: the Java DebugContext adds a `debug(Object)` method to the context, which JEXL reaches
// by reflecting on the context object; this port cannot call a method on a Rust context, so the
// identity call is inlined. The JVM answers 42 for both forms.
#[test]
fn test_lexical5() {
    let jexl = builder().strict(true).lexical(true).create();
    let ctxt: Arc<dyn JexlContext> = Arc::new(MapContext::new());
    let script = jexl
        .create_script("var x = 42; var y = () -> { {var x = -42; }; return x; }; y()")
        .expect("parse");
    eq(&ok(script.execute(ctxt)), &i(42));
}

// port of: LexicalTest.testLexical6a
#[test]
fn test_lexical6a() {
    let e = builder()
        .strict(true)
        .lexical(true)
        .create()
        .create_script("i = 0; { var i = 32; }; i")
        .expect("parse");
    eq(&ok(e.execute(Arc::new(MapContext::new()))), &i(0));
}

// port of: LexicalTest.testLexical6b
#[test]
fn test_lexical6b() {
    let e = builder()
        .strict(true)
        .lexical(true)
        .lexical_shade(true)
        .create()
        .create_script("i = 0; { var i = 32; }; i")
        .expect("parse");
    let x = thrown(e.execute(Arc::new(MapContext::new())));
    assert_eq!(x.class_name(), "JexlException$Variable");
}

// port of: LexicalTest.testLexical6c
#[test]
fn test_lexical6c() {
    let e = builder()
        .strict(true)
        .lexical(true)
        .lexical_shade(false)
        .create()
        .create_script("i = 0; for (var i : [42]) i; i")
        .expect("parse");
    eq(&ok(e.execute(Arc::new(MapContext::new()))), &i(0));
}

// port of: LexicalTest.testLexical6d
#[test]
fn test_lexical6d() {
    let e = builder()
        .strict(true)
        .lexical(true)
        .lexical_shade(true)
        .create()
        .create_script("i = 0; for (var i : [42]) i; i")
        .expect("parse");
    let x = thrown(e.execute(Arc::new(MapContext::new())));
    assert_eq!(x.class_name(), "JexlException$Variable");
}

// port of: LexicalTest.testPragmaOptions
#[test]
fn test_pragma_options() {
    // same as 6d but using a pragma
    let str_ = "#pragma jexl.options '+strict +lexical +lexicalShade -safe'\ni = 0; for (var i : [42]) i; i";
    let e = builder().strict(false).create().create_script(str_).expect("parse");
    let x = thrown(e.execute(Arc::new(MapContext::new())));
    assert_eq!(x.class_name(), "JexlException$Variable");
}

// port of: LexicalTest.testPragmaNoop
#[test]
fn test_pragma_noop() {
    // unknown pragma
    let str_ = "#pragma jexl.options 'no effect'\ni = -42; for (var i : [42]) i; i";
    let e = builder().lexical(false).strict(true).create().create_script(str_).expect("parse");
    eq(&ok(e.execute(Arc::new(MapContext::new()))), &i(42));
}

// port of: LexicalTest.testScopeFrame
#[test]
fn test_scope_frame() {
    let mut scope = LexicalScope::new();
    for n in (0..128).step_by(2) {
        assert!(scope.add_symbol(n));
        assert!(!scope.add_symbol(n));
    }
    for n in (0..128).step_by(2) {
        assert!(scope.has_symbol(n));
        assert!(!scope.has_symbol(n + 1));
    }
}

// port of: LexicalTest.testParameter0
#[test]
fn test_parameter0() {
    let str_ = "function(u) {}";
    let jexl = builder().create();
    let e = jexl.create_script(str_).expect("parse");
    assert_eq!(e.get_parameters().len(), 1);
    let e = jexl
        .create_script_info(Some(rust_jexl::jexl_info::JexlInfo::new(Some("TestScript".into()), 1, 1)), str_, None)
        .expect("parse");
    assert_eq!(e.get_parameters().len(), 1);
}

// port of: LexicalTest.testParameter1
#[test]
fn test_parameter1() {
    let jexl = builder().strict(true).lexical(true).create();
    let jc: Arc<dyn JexlContext> = Arc::new(MapContext::new());
    let strs = "var s = function(x) { for (var i : 1..3) {if (i > 2) return x}}; s(42)";
    let s42 = jexl.create_script(strs).expect("parse");
    eq(&ok(s42.execute(jc)), &i(42));
}

// port of: LexicalTest.testInnerAccess0
#[test]
fn test_inner_access0() {
    let f = JexlFeatures::new().lexical(true);
    let jexl = builder().strict(true).features(f).create();
    let script = jexl
        .create_script("var x = 32; (()->{ for(var x : null) { var c = 0; {return x; }} })();")
        .expect("parse");
    eq(&ok(script.execute(empty_context())), &Value::Null);
}

// port of: LexicalTest.testInnerAccess1
#[test]
fn test_inner_access1() {
    let jexl = builder().strict(true).lexical(true).create();
    jexl.create_script("var x = 32; (()->{ for(var x : null) { var c = 0; {return x; }} })();")
        .expect("parse");
}

fn lexical_shade_features() -> JexlFeatures {
    JexlFeatures::new().lexical(true).lexical_shade(true)
}

// port of: LexicalTest.testForVariable0
#[test]
fn test_for_variable0() {
    let jexl = builder().strict(true).features(lexical_shade_features()).create();
    let x = parse_err(jexl.create_script("for(var x : 1..3) { var c = 0}; return x"));
    assert!(x.is_jexl(), "{}", x.message());
}

// port of: LexicalTest.testForVariable1
#[test]
fn test_for_variable1() {
    let jexl = builder().strict(true).features(lexical_shade_features()).create();
    let x = parse_err(jexl.create_script("for(var x : 1..3) { var c = 0} for(var x : 1..3) { var c = 0}; return x"));
    assert!(x.is_jexl(), "{}", x.message());
}

// port of: LexicalTest.testUndeclaredVariable
#[test]
fn test_undeclared_variable() {
    let jexl = builder().strict(true).features(lexical_shade_features()).create();
    let x = parse_err(jexl.create_script("{var x = 0}; return x"));
    assert!(x.is_jexl(), "{}", x.message());
}

// port of: LexicalTest.testLexical6a1
#[test]
fn test_lexical6a1() {
    let f = JexlFeatures::new().lexical(true);
    let e = builder()
        .strict(true)
        .features(f)
        .create()
        .create_script("i = 0; { var i = 32; }; i")
        .expect("parse");
    eq(&ok(e.execute(Arc::new(MapContext::new()))), &i(0));
}

// port of: LexicalTest.testOptionsPragma
// The Java test runs under `JexlOptions.setDefaultFlags("+safe", "-lexical", "-lexicalShade")`,
// which is the library's own default mask; this port has no process-wide default, so the plain
// `JexlBuilder`/`JexlOptions` constructors are it.
#[test]
fn test_options_pragma() {
    let vars = EvalContext::var_context();
    let jexl = hosts(JexlBuilder::new()).create();

    let n42 = ok(jexl.create_script("#pragma jexl.options none\n-42").expect("parse").execute(vars.clone()));
    eq(&n42, &i(-42));
    let o = vars.snatch_options();
    assert!(o.is_strict());
    assert!(o.is_safe());
    assert!(o.is_cancellable());
    assert!(!o.is_lexical());
    assert!(!o.is_lexical_shade());

    let n42 = ok(jexl.create_script("#pragma jexl.options canonical\n42").expect("parse").execute(vars.clone()));
    eq(&n42, &i(42));
    let o = vars.snatch_options();
    assert!(o.is_strict());
    assert!(!o.is_safe());
    assert!(o.is_cancellable());
    assert!(o.is_lexical());
    assert!(o.is_lexical_shade());
    assert!(!o.is_shared_instance());
}

/// port of: LexicalTest.runVarLoop
fn run_var_loop(flag: bool, src: &str) -> JexlFeatures {
    let vars = EvalContext::var_context();
    vars.options(|o| {
        o.set_lexical(true);
        o.set_lexical_shade(true);
        o.set_safe(false);
    });
    let mut features = JexlFeatures::new();
    if flag {
        features = features.lexical(true).lexical_shade(true);
    }
    let jexl = hosts(JexlBuilder::new()).features(features.clone()).create();
    let script = jexl.create_script(src).expect("parse");
    let out = list(vec![]);
    vars.set("$out", out.clone()).expect("set");
    let result = ok(script.execute(vars));
    eq(&result, &Value::Boolean(true));
    assert_eq!(match &out { Value::List(l) => l.len(), _ => 0 }, 10);
    features
}

// port of: LexicalTest.testVarLoop0
#[test]
fn test_var_loop0() {
    let src0 = "var count = 10;\nfor (var i : 0 .. count-1) {\n  $out.add(i);\n}";
    let src1 = "var count = [0,1,2,3,4,5,6,7,8,9];\nfor (var i : count) {\n  $out.add(i);\n}";
    let src2 = "var count = 10;\n  var outer = 0;\nfor (var i : 0 .. count-1) {\n  $out.add(i);\n  outer = i;}\nouter == 9";
    let ff0 = run_var_loop(false, src0);
    let ft0 = run_var_loop(true, src0);
    let ff1 = run_var_loop(false, src1);
    let ft1 = run_var_loop(true, src1);
    run_var_loop(false, src2);
    run_var_loop(true, src2);

    // and check some features features
    assert_eq!(ff0, ff1);
    assert_eq!(ft0, ft1);
    assert_ne!(ff0, ft0);
    // Java's JexlFeatures has no toString() override, so the strings it compares are
    // "JexlFeatures@" plus the overridden hashCode, i.e. equality of flags and reserved names.
    let sff0 = format!("{:?}", ff0);
    let sff1 = format!("{:?}", ff1);
    assert_eq!(sff0, sff1);
    let sft1 = format!("{:?}", ft1);
    assert_ne!(sff0, sft1);
}

// port of: LexicalTest.testAnnotation
#[test]
fn test_annotation() {
    let f = JexlFeatures::new().lexical(true);
    let jexl = builder().strict(true).features(f).create();
    let script = jexl.create_script("@scale(13) @test var i = 42").expect("parse");
    let jc = EvalContext::annotating();
    eq(&ok(script.execute(jc)), &i(42));
}

// port of: LexicalTest.testNamed
#[test]
fn test_named() {
    let f = JexlFeatures::new().lexical(true);
    let jexl = builder().strict(true).features(f).create();
    let script = jexl.create_script("var i = (x, y, z)->{return x + y + z}; i(22,18,2)").expect("parse");
    eq(&ok(script.execute(Arc::new(MapContext::new()))), &i(42));
}

// port of: LexicalTest.tesstCaptured0 (the upstream method name is misspelled)
#[test]
fn tesst_captured0() {
    let f = JexlFeatures::new().lexical(true);
    let jexl = builder().strict(true).features(f).create();
    let script = jexl.create_script("var x = 10; (b->{ x + b })(32)").expect("parse");
    eq(&ok(script.execute(Arc::new(MapContext::new()))), &i(42));
}

// port of: LexicalTest.testCaptured1
#[test]
fn test_captured1() {
    let f = JexlFeatures::new().lexical(true);
    let jexl = builder().strict(true).features(f).create();
    let script = jexl.create_script("{var x = 10; } (b->{ x + b })(32)").expect("parse");
    let jc = MapContext::new();
    jc.set("x", i(11)).expect("set");
    eq(&ok(script.execute(Arc::new(jc))), &i(43));
}

// ================================================================== VarTest

// port of: VarTest.testStrict
#[test]
fn test_strict() {
    let env = EvalContext::new();
    let ctxt: Arc<dyn JexlContext> = Arc::new(ReadonlyContext(env.clone()));
    env.options(|o| {
        o.set_strict(true);
        o.set_silent(false);
        o.set_safe(false);
    });
    let jexl = jexl();

    let e = jexl.create_script("x").expect("parse");
    // ok since we are strict and x does not exist
    thrown(e.execute(ctxt.clone()));

    let e = jexl.create_script("x = 42").expect("parse");
    // ok since we are strict and context is readonly
    thrown(e.execute(ctxt.clone()));

    env.set("x", s("fourty-two")).expect("set");
    let e = jexl.create_script("x.theAnswerToEverything()").expect("parse");
    // ok since we are strict and method does not exist
    thrown(e.execute(ctxt));
}

// port of: VarTest.testLocalBasic
#[test]
fn test_local_basic() {
    let e = jexl().create_script("var x; x = 42").expect("parse");
    eq(&ok(e.execute(empty_context())), &i(42));
}

// port of: VarTest.testLocalSimple
#[test]
fn test_local_simple() {
    let e = jexl().create_script("var x = 21; x + x").expect("parse");
    eq(&ok(e.execute(empty_context())), &i(42));
}

// port of: VarTest.testLocalFor
#[test]
fn test_local_for() {
    let e = jexl()
        .create_script("var y  = 0; for(var x : [5, 17, 20]) { y = y + x; } y;")
        .expect("parse");
    eq(&ok(e.execute(empty_context())), &i(42));
}

// port of: VarTest.testLocalForFunc
#[test]
fn test_local_for_func() {
    let jc = EvalContext::with_null_namespace(Value::object(Numbers));
    let e = jexl()
        .create_script("var y  = 0; for(var x : numbers()) { y = y + x; } y;")
        .expect("parse");
    eq(&ok(e.execute(jc)), &i(42));
}

// port of: VarTest.testLocalForFuncReturn
#[test]
fn test_local_for_func_return() {
    let jc = EvalContext::with_null_namespace(Value::object(Numbers));
    let e = jexl()
        .create_script("var y  = 42; for(var x : numbers()) { if (x > 10) return x } y;")
        .expect("parse");
    eq(&ok(e.execute(jc)), &i(17));
    assert!(e.get_variables().is_empty(), "{:?}", e.get_variables());
}

// port of: VarTest.testRefs
#[test]
fn test_refs() {
    let jexl = jexl();
    let cases: Vec<(&str, Vec<&[&str]>)> = vec![
        ("a[b]['c']", vec![&["a"], &["b"]]),
        ("a.'b + c'", vec![&["a", "b + c"]]),
        ("e[f]", vec![&["e"], &["f"]]),
        ("e[f][g]", vec![&["e"], &["f"], &["g"]]),
        ("e['f'].goo", vec![&["e", "f", "goo"]]),
        ("e['f']", vec![&["e", "f"]]),
        ("e[f]['g']", vec![&["e"], &["f"]]),
        ("e['f']['g']", vec![&["e", "f", "g"]]),
        ("a['b'].c['d'].e", vec![&["a", "b", "c", "d", "e"]]),
        ("a + b.c + b.c.d + e['f']", vec![&["a"], &["b", "c"], &["b", "c", "d"], &["e", "f"]]),
        ("D[E[F]]", vec![&["D"], &["E"], &["F"]]),
        ("D[E[F[G[H]]]]", vec![&["D"], &["E"], &["F"], &["G"], &["H"]]),
        (
            " A + B[C] + D[E[F]] + x[y[z]] ",
            vec![&["A"], &["B"], &["C"], &["D"], &["E"], &["F"], &["x"], &["y"], &["z"]],
        ),
        (" A + B[C] + D.E['F'] + x[y.z] ", vec![&["A"], &["B"], &["C"], &["D", "E", "F"], &["x"], &["y", "z"]]),
        ("(A)", vec![&["A"]]),
        ("not(A)", vec![&["A"]]),
        ("not((A))", vec![&["A"]]),
        ("a[b]['c']", vec![&["a"], &["b"]]),
        ("a['b'][c]", vec![&["a", "b"], &["c"]]),
        ("a[b].c", vec![&["a"], &["b"]]),
        ("a[b].c[d]", vec![&["a"], &["b"], &["d"]]),
        ("a[b][e].c[d][f]", vec![&["a"], &["b"], &["d"], &["e"], &["f"]]),
    ];
    for (src, expect) in cases {
        let e = jexl.create_script(src).expect("parse");
        assert_eq!(vars_of(&e), refs(&expect), "{}", src);
    }
}

// port of: VarTest.testVarCollectNotAll
#[test]
fn test_var_collect_not_all() {
    // collectAll(false) is collectMode 0
    let jexl = builder().strict(true).silent(false).cache(32).collect_mode(0).create();
    let cases: Vec<(&str, Vec<&[&str]>)> = vec![
        ("a['b'][c]", vec![&["a"], &["c"]]),
        (
            " A + B[C] + D[E[F]] + x[y[z]] ",
            vec![&["A"], &["B"], &["C"], &["D"], &["E"], &["F"], &["x"], &["y"], &["z"]],
        ),
        ("e['f']['g']", vec![&["e"]]),
        ("a[b][e].c[d][f]", vec![&["a"], &["b"], &["d"], &["e"], &["f"]]),
        ("a + b.c + b.c.d + e['f']", vec![&["a"], &["b", "c"], &["b", "c", "d"], &["e"]]),
    ];
    for (src, expect) in cases {
        let e = jexl.create_script(src).expect("parse");
        assert_eq!(vars_of(&e), refs(&expect), "{}", src);
    }
}

// port of: VarTest.testMix
#[test]
fn test_mix() {
    // x is a parameter, y a context variable, z a local variable
    let e = jexl()
        .create_script_named("if (x) { y } else { var z = 2 * x}", &["x".into()])
        .expect("parse");
    assert_eq!(vars_of(&e), refs(&[&["y"]]));
    let parms = e.get_parameters();
    let locals = e.get_local_variables();
    assert_eq!(parms.len(), 1);
    assert_eq!(parms[0], "x");
    assert_eq!(locals.len(), 1);
    assert_eq!(locals[0], "z");
}

// port of: VarTest.testSyntacticVariations
#[test]
fn test_syntactic_variations() {
    let script = jexl()
        .create_script("sum(TOTAL) - partial.sum() + partial['sub'].avg() - sum(partial.sub)")
        .expect("parse");
    assert_eq!(script.get_variables().len(), 3);
}

// ================================================================== LambdaTest

// port of: LambdaTest.testLambda
#[test]
fn test_lambda() {
    let jexl = create_engine();
    let s42 = jexl.create_script("var s = function(x) { x + x }; s(21)").expect("parse");
    eq(&ok(s42.execute(empty_context())), &i(42));
    let s42 = jexl.create_script("var s = function(x, y) { x + y }; s(15, 27)").expect("parse");
    eq(&ok(s42.execute(empty_context())), &i(42));
}

// port of: LambdaTest.testLambdaClosure
#[test]
fn test_lambda_closure() {
    let jexl = create_engine();
    for strs in [
        "var t = 20; var s = function(x, y) { x + y + t}; s(15, 7)",
        "var t = 19; var s = function(x, y) { var t = 20; x + y + t}; s(15, 7)",
        "var t = 20; var s = function(x, y) {x + y + t}; t = 54; s(15, 7)",
        "var t = 19; var s = function(x, y) { var t = 20; x + y + t}; t = 54; s(15, 7)",
    ] {
        let s42 = jexl.create_script(strs).expect("parse");
        eq(&ok(s42.execute(empty_context())), &i(42));
    }
}

// port of: LambdaTest.testLambdaLambda
#[test]
fn test_lambda_lambda() {
    let jexl = create_engine();
    for strs in [
        "var t = 19; ( (x, y)->{ var t = 20; x + y + t} )(15, 7);",
        "( (x, y)->{ ( (xx, yy)->{xx + yy } )(x, y) } )(15, 27)",
        "var t = 19; var s = (x, y)->{ var t = 20; x + y + t}; t = 54; s(15, 7)",
    ] {
        let s42 = jexl.create_script(strs).expect("parse");
        eq(&ok(s42.execute(empty_context())), &i(42));
    }
}

// port of: LambdaTest.testNestLambda
#[test]
fn test_nest_lambda() {
    let s42 = create_engine().create_script("( (x)->{ (y)->{ x + y } })(15)(27)").expect("parse");
    eq(&ok(s42.execute(empty_context())), &i(42));
}

// port of: LambdaTest.testRecurse
#[test]
fn test_recurse() {
    let jc: Arc<dyn JexlContext> = Arc::new(MapContext::new());
    let script = create_engine()
        .create_script("var fact = (x)->{ if (x <= 1) 1; else x * fact(x - 1) }; fact(5)")
        .expect("parse");
    eq(&ok(script.execute(jc)), &i(120));
}

// port of: LambdaTest.testRecurse2
#[test]
fn test_recurse2() {
    let jc: Arc<dyn JexlContext> = Arc::new(MapContext::new());
    // adding some captured vars to get it confused
    let script = create_engine()
        .create_script("var y = 1; var z = 1; var fact = (x)->{ if (x <= y) z; else x * fact(x - 1) }; fact(6)")
        .expect("parse");
    eq(&ok(script.execute(jc)), &i(720));
}

// port of: LambdaTest.testRecurse3
#[test]
fn test_recurse3() {
    let jc: Arc<dyn JexlContext> = Arc::new(MapContext::new());
    // adding some captured vars to get it confused
    let script = create_engine()
        .create_script(
            "var y = 1; var z = 1;var foo = (x)->{y + z}; var fact = (x)->{ if (x <= y) z; else x * fact(x - 1) }; fact(6)",
        )
        .expect("parse");
    eq(&ok(script.execute(jc)), &i(720));
}

// port of: LambdaTest.testIdentity
#[test]
fn test_identity() {
    let script = create_engine().create_script("(x)->{ x }").expect("parse");
    assert_eq!(script.get_parameters(), vec!["x".to_string()]);
    eq(&ok(script.execute_args(empty_context(), &[i(42)])), &i(42));
}

// port of: LambdaTest.test271a
#[test]
fn test271a() {
    let base = create_engine()
        .create_script("var base = 1; var x = (a)->{ var y = (b) -> {base + b}; return base + y(a)}; x(40)")
        .expect("parse");
    eq(&ok(base.execute(empty_context())), &i(42));
}

// ================================================================== AnnotationTest

// port of: AnnotationTest.test197a
#[test]
fn test197a() {
    let jc: Arc<dyn JexlContext> = Arc::new(MapContext::new());
    let e = jexl().create_script("@synchronized { return 42; }").expect("parse");
    eq(&ok(e.execute(jc)), &i(42));
}

// port of: AnnotationTest.testNoArg
#[test]
fn test_no_arg() {
    let jc = EvalContext::annotating();
    let e = jexl().create_script("@synchronized { return 42; }").expect("parse");
    eq(&ok(e.execute(jc.clone())), &i(42));
    assert_eq!(jc.count(), 1);
    assert!(jc.names().contains("synchronized"));
}

// port of: AnnotationTest.testNoArgExpression
#[test]
fn test_no_arg_expression() {
    let jc = EvalContext::annotating();
    let e = jexl().create_script("@synchronized 42").expect("parse");
    eq(&ok(e.execute(jc.clone())), &i(42));
    assert_eq!(jc.count(), 1);
    assert!(jc.names().contains("synchronized"));
}

// port of: AnnotationTest.testNoArgStatement
#[test]
fn test_no_arg_statement() {
    let jc = EvalContext::annotating();
    let e = jexl().create_script("@synchronized if (true) 2 * 3 * 7; else -42;").expect("parse");
    eq(&ok(e.execute(jc.clone())), &i(42));
    assert_eq!(jc.count(), 1);
    assert!(jc.names().contains("synchronized"));
}

// port of: AnnotationTest.testHoistingStatement
#[test]
fn test_hoisting_statement() {
    let jc = EvalContext::annotating();
    let e = jexl()
        .create_script("var t = 1; @synchronized for(var x : [2,3,7]) t *= x; t")
        .expect("parse");
    eq(&ok(e.execute(jc.clone())), &i(42));
    assert_eq!(jc.count(), 1);
    assert!(jc.names().contains("synchronized"));
}

// port of: AnnotationTest.testOneArg
#[test]
fn test_one_arg() {
    let jc = EvalContext::annotating();
    let e = jexl().create_script("@one(1) { return 42; }").expect("parse");
    eq(&ok(e.execute(jc.clone())), &i(42));
    assert_eq!(jc.count(), 1);
    assert!(jc.names().contains("one"));
    assert!(jc.names().contains("1"));
}

// port of: AnnotationTest.testMultiple
#[test]
fn test_multiple() {
    let jc = EvalContext::annotating();
    let e = jexl().create_script("@one(1) @synchronized { return 42; }").expect("parse");
    eq(&ok(e.execute(jc.clone())), &i(42));
    assert_eq!(jc.count(), 2);
    assert!(jc.names().contains("synchronized"));
    assert!(jc.names().contains("one"));
    assert!(jc.names().contains("1"));
}

// ================================================================== PragmaTest

// port of: PragmaTest.testPragmas
#[test]
fn test_pragmas() {
    let script = jexl()
        .create_script("#pragma one 1\n#pragma the.very.hard 'truth'\n2;")
        .expect("parse");
    let pragmas = script.get_pragmas();
    match &pragmas {
        Value::Map(m) => {
            assert_eq!(m.len(), 2);
            eq(&m.get(&s("one")).expect("one"), &i(1));
            eq(&m.get(&s("the.very.hard")).expect("truth"), &s("truth"));
        }
        other => panic!("not a map: {:?}", other),
    }
}

// port of: PragmaTest.testJxltPragmas
#[test]
fn test_jxlt_pragmas() {
    let engine = rust_jexl::jxlt_engine::create_jxlt_engine(&builder().create());
    let tscript = engine
        .create_template_str("$$ #pragma one 1\n$$ #pragma the.very.hard 'truth'\n2;", None)
        .expect("template");
    let pragmas = tscript.get_pragmas();
    match &pragmas {
        Value::Map(m) => {
            assert_eq!(m.len(), 2);
            eq(&m.get(&s("one")).expect("one"), &i(1));
            eq(&m.get(&s("the.very.hard")).expect("truth"), &s("truth"));
        }
        other => panic!("not a map: {:?}", other),
    }
}

/// port of: PragmaTest.SafeContext.processPragmas
fn process_pragmas(jc: &EvalContext, pragmas: &Value) {
    if let Value::Map(m) = pragmas {
        for (key, value) in m.snapshot() {
            match (key.java_to_string().as_str(), &value) {
                ("jexl.safe", Value::Boolean(b)) => jc.options(|o| o.set_safe(*b)),
                ("jexl.strict", Value::Boolean(b)) => jc.options(|o| o.set_strict(*b)),
                ("jexl.silent", Value::Boolean(b)) => jc.options(|o| o.set_silent(*b)),
                _ => {}
            }
        }
    }
}

// port of: PragmaTest.testSafePragma
#[test]
fn test_safe_pragma() {
    let jc = EvalContext::new();
    jc.set("foo", Value::Null).expect("set");
    let script = jexl().create_script("#pragma jexl.safe true\nfoo.bar;").expect("parse");
    process_pragmas(&jc, &script.get_pragmas());
    eq(&ok(script.execute(jc)), &Value::Null);

    let jc = EvalContext::new();
    jc.set("foo", Value::Null).expect("set");
    let x = thrown(script.execute(jc));
    assert!(x.is_jexl(), "{}", x.message());
}

