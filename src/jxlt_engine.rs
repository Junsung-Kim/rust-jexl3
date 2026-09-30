//! port of: org.apache.commons.jexl3.JxltEngine — the public face of the JEXL template engine.
//!
//! Java's `JxltEngine` is an abstract class with two nested interfaces, `Expression` and
//! `Template`, and one implementation, `internal.TemplateEngine`. There is only ever that one
//! implementation, so this port drops the abstraction: `JxltEngine` is
//! [`crate::internal::template_engine::TemplateEngine`], `Expression` is
//! [`crate::internal::template_engine::TemplateExpression`] and `Template` is
//! [`crate::internal::template_script::TemplateScript`]. What lives here is the rest of the public
//! class: the factory methods (including the `createTemplate` overloads that fill in the `"$$"`
//! prefix) and `JxltEngine.Exception`.
//!
//! ```text
//! JxltEngine.Expression            TemplateExpression
//!   asString()                       as_string() -> JString
//!   asString(StringBuilder)          as_string_into(&mut JStringBuilder)
//!   evaluate(JexlContext)            evaluate(Arc<dyn JexlContext>)
//!   getSource()                      get_source()
//!   getVariables()                   get_variables()
//!   isDeferred() / isImmediate()     is_deferred() / is_immediate()
//!   prepare(JexlContext)             prepare(Arc<dyn JexlContext>)   -> Ok(None) is Java's null
//!   toString()                       java_to_jstring()
//!
//! JxltEngine.Template              TemplateScript
//!   asString()                       as_string()
//!   evaluate(ctx, writer, args)      evaluate(ctx, Option<Arc<StringWriter>>, &[Value])
//!   prepare(JexlContext)             prepare(ctx)                    -> Ok(None) is Java's null
//!   getVariables() / getParameters() / getPragmas()   same names
//! ```
use std::sync::Arc;

use crate::internal::template_engine::{TemplateEngine, TemplateExpression};
use crate::internal::template_script::TemplateScript;
use crate::java::string::JString;
use crate::jexl_engine::JexlEngine;
use crate::jexl_exception::JexlException;
use crate::jexl_info::JexlInfo;

pub use crate::internal::template_engine::TemplateExpression as Expression;
pub use crate::internal::template_script::TemplateScript as Template;

/// port of: JexlEngine.JXLT_CACHE_SIZE
pub const JXLT_CACHE_SIZE: i32 = 256;

/// port of: JexlEngine.createJxltEngine()
pub fn create_jxlt_engine(jexl: &Arc<JexlEngine>) -> Arc<TemplateEngine> {
    create_jxlt_engine_no_script(jexl, true)
}

/// port of: JexlEngine.createJxltEngine(boolean)
pub fn create_jxlt_engine_no_script(jexl: &Arc<JexlEngine>, noscript: bool) -> Arc<TemplateEngine> {
    create_jxlt_engine_with(jexl, noscript, JXLT_CACHE_SIZE, '$', '#')
}

/// port of: Engine.createJxltEngine(boolean, int, char, char)
pub fn create_jxlt_engine_with(
    jexl: &Arc<JexlEngine>,
    noscript: bool,
    cache_size: i32,
    immediate: char,
    deferred: char,
) -> Arc<TemplateEngine> {
    Arc::new(TemplateEngine::new(jexl.clone(), noscript, cache_size, immediate, deferred))
}

/// port of: JxltEngine.Exception(JexlInfo, String, Throwable)
pub fn exception(info: Option<JexlInfo>, msg: &str, cause: Option<JexlException>) -> JexlException {
    JexlException::jxlt(info, msg, cause)
}

impl TemplateEngine {
    /// port of: JxltEngine.createExpression(String)
    pub fn create_expression_str(
        self: &Arc<Self>,
        expression: &str,
    ) -> Result<Option<Arc<TemplateExpression>>, JexlException> {
        self.create_expression(None, &JString::from(expression))
    }

    /// port of: JxltEngine.createTemplate(JexlInfo, String, String...)
    pub fn create_template_info(
        self: &Arc<Self>,
        info: Option<JexlInfo>,
        source: &str,
        parms: Option<&[String]>,
    ) -> Result<Arc<TemplateScript>, JexlException> {
        self.create_template(info, "$$", &JString::from(source), parms)
    }

    /// port of: JxltEngine.createTemplate(String, String...)
    pub fn create_template_str(
        self: &Arc<Self>,
        source: &str,
        parms: Option<&[String]>,
    ) -> Result<Arc<TemplateScript>, JexlException> {
        self.create_template(None, "$$", &JString::from(source), parms)
    }
}
