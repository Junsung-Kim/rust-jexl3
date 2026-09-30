// port of: org.apache.commons.jexl3.JexlInfo (and org.apache.commons.jexl3.parser.JexlNode.Info)

use crate::java::string::{JString, JStringBuilder};

/// The detailed information about a sub-expression (JexlInfo.Detail), rendered by the Debugger.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Detail {
    pub start: i32,
    pub end: i32,
    pub text: JString,
}

/// Helper class to carry information such as a url/file name, line and column for debugging
/// information reporting.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JexlInfo {
    name: Option<String>,
    line: i32,
    column: i32,
    detail: Option<Detail>,
    /// JexlNode.Info: the node this info was created from (template expressions)
    pub(crate) node: Option<crate::parser::jexl_node::NodeHandle>,
}

impl JexlInfo {
    // port of: JexlInfo(String, int, int)
    pub fn new(source: impl Into<Option<String>>, line: i32, column: i32) -> JexlInfo {
        JexlInfo { name: source.into(), line, column, detail: None, node: None }
    }

    // port of: JexlInfo() — Java derives the name from the caller's stack frame
    // ("class.method:line"); Rust has no equivalent, so the caller location is used instead.
    #[track_caller]
    pub fn from_caller() -> JexlInfo {
        JexlInfo::at_location(std::panic::Location::caller())
    }

    pub(crate) fn at_location(loc: &std::panic::Location<'_>) -> JexlInfo {
        JexlInfo::new(format!("{}:{}", loc.file(), loc.line()), 0, 0)
    }

    // port of: JexlInfo.at(int, int)
    pub fn at(&self, line: i32, column: i32) -> JexlInfo {
        JexlInfo { name: self.name.clone(), line, column, detail: None, node: self.node.clone() }
    }

    // port of: JexlInfo.getDetail
    pub fn get_detail(&self) -> Option<&Detail> {
        self.detail.as_ref()
    }

    pub(crate) fn with_detail(&self, d: Detail) -> JexlInfo {
        JexlInfo { detail: Some(d), ..self.clone() }
    }

    pub fn get_name(&self) -> Option<&str> {
        self.name.as_deref()
    }
    pub fn get_line(&self) -> i32 {
        self.line
    }
    pub fn get_column(&self) -> i32 {
        self.column
    }

    // port of: JexlInfo.detach / JexlNode.Info.detach
    // `JexlNode.Info.detach()` returns `node.jexlInfo()`, not a copy of itself: a template
    // sub-expression parsed with a JexlNode.Info therefore carries the *node's* position.
    pub fn detach(&self) -> JexlInfo {
        if let Some(n) = &self.node {
            return n.node().jexl_info().unwrap_or_else(|| JexlInfo::new(None, 0, 0));
        }
        JexlInfo { node: None, ..self.clone() }
    }

    // port of: JexlNode.Info(JexlNode, JexlInfo)
    pub(crate) fn with_node(&self, node: crate::parser::jexl_node::NodeHandle) -> JexlInfo {
        JexlInfo { node: Some(node), ..self.clone() }
    }
}

impl JexlInfo {
    // port of: JexlInfo.toString
    pub fn to_jstring(&self) -> JString {
        let mut sb = JStringBuilder::new();
        sb.str(self.name.as_deref().unwrap_or(""));
        if self.line > 0 {
            sb.str("@").str(&self.line.to_string());
            if self.column > 0 {
                sb.str(":").str(&self.column.to_string());
            }
        }
        if let Some(d) = &self.detail {
            sb.str("![")
                .str(&d.start.to_string())
                .str(",")
                .str(&d.end.to_string())
                .str("]: '")
                .jstr(&d.text)
                .str("'");
        }
        sb.build()
    }
}

impl std::fmt::Display for JexlInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.to_jstring().to_rust())
    }
}
