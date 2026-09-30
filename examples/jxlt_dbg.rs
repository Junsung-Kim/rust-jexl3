use std::sync::Arc;
use rust_jexl::jexl_context::{JexlContext, MapContext};
use rust_jexl::jexl_engine::JexlBuilder;
use rust_jexl::java::string::JString;
use rust_jexl::jexl_info::JexlInfo;
use rust_jexl::internal::template_interpreter::StringWriter;
use rust_jexl::value::Value;

fn main() {
    let engine = JexlBuilder::new().create();
    let jxlt = rust_jexl::jxlt_engine::create_jxlt_engine(&engine);
    let ctx = Arc::new(MapContext::new());
    let info = JexlInfo::new(Some("case".into()), 1, 1);
    let parms = vec!["p0".to_string(), "p1".to_string()];
    let t = jxlt.create_template(Some(info), "$$", &JString::from("${p0}${p1}${p0}\n"), Some(&parms)).unwrap();
    println!("asString={:?}", t.as_string().to_rust());
    let w = StringWriter::new();
    let args = vec![Value::Integer(1), Value::string("a")];
    match t.evaluate(ctx.clone(), Some(w.clone()), &args) {
        Ok(()) => println!("out={:?}", w.to_jstring().to_rust()),
        Err(e) => println!("err={}", e.message()),
    }
    println!("params={:?}", t.get_parameters());
}
