// port of: org.apache.commons.jexl3.JexlException (the whole hierarchy as one type)
//
// Java throws either JexlException subclasses or, in a few places, raw JDK exceptions
// (PatternSyntaxException from a regex literal, NumberFormatException from a numeric literal, ...).
// Both are represented here: `ExceptionKind::Java` carries the Java class name of a non-JEXL
// throwable, so `class_name()` and `get_message()` stay identical to Java's.
use std::fmt;

use crate::java::string::{JString, JStringBuilder};
use crate::jexl_features::JexlFeatures;
use crate::jexl_info::JexlInfo;
use crate::parser::jexl_node::NodeHandle;
use crate::value::Value;

/// The variable issues (JexlException.VariableIssue).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VariableIssue {
    Undefined,
    Redefined,
    NullValue,
}

impl VariableIssue {
    // port of: JexlException.VariableIssue.message
    pub fn message(&self, var: &JString) -> JString {
        let tail = match self {
            VariableIssue::NullValue => "' is null",
            VariableIssue::Redefined => "' is already defined",
            VariableIssue::Undefined => "' is undefined",
        };
        JStringBuilder::new().str("variable '").jstr(var).str(tail).build()
    }
}

/// One variant per JexlException subclass (plus non-JEXL Java throwables).
#[derive(Clone, Debug)]
pub enum ExceptionKind {
    /// org.apache.commons.jexl3.JexlException itself
    Jexl,
    Tokenization,
    Parsing,
    Ambiguous { recover: Option<JexlInfo> },
    StackOverflow,
    Assignment,
    Feature { code: i32 },
    Variable { issue: VariableIssue },
    Property { undefined: bool },
    Method,
    Operator,
    Annotation,
    Return { value: Value },
    Cancel,
    Break,
    Continue,
    TryFailed,
    /// org.apache.commons.jexl3.JxltEngine.Exception
    Jxlt,
    /// a non-JEXL throwable, by Java class name (e.g. "java.lang.ArithmeticException")
    Java { class: String },
}

/// A Jexl exception (or a Java throwable that JEXL lets escape), with its cause chain.
#[derive(Clone)]
pub struct JexlException {
    kind: ExceptionKind,
    /// the node the exception is attached to (JexlException.mark)
    mark: Option<NodeHandle>,
    info: Option<JexlInfo>,
    /// Throwable.getMessage() of the super class (JexlException.getDetail); None is Java null
    detail: Option<JString>,
    cause: Option<Box<JexlException>>,
}

const MAX_EXCHARLOC: usize = 42;

impl JexlException {
    fn build(kind: ExceptionKind, mark: Option<NodeHandle>, info: Option<JexlInfo>, detail: Option<JString>, cause: Option<JexlException>) -> Self {
        JexlException { kind, mark, info, detail, cause: Self::unwrap(cause).map(Box::new) }
    }

    // port of: JexlException.unwrap
    fn unwrap(cause: Option<JexlException>) -> Option<JexlException> {
        match cause {
            Some(c) if matches!(c.kind, ExceptionKind::TryFailed) => c.cause.map(|b| *b),
            c => c,
        }
    }

    fn node_info(node: &Option<NodeHandle>) -> Option<JexlInfo> {
        node.as_ref().and_then(|n| n.node().jexl_info())
    }

    // port of: JexlException(JexlNode, String, Throwable)
    pub fn new(node: Option<NodeHandle>, msg: &str, cause: Option<JexlException>) -> Self {
        let info = Self::node_info(&node);
        Self::build(ExceptionKind::Jexl, node, info, Some(JString::from(msg)), cause)
    }

    // port of: JexlException(JexlInfo, String, Throwable)
    pub fn with_info(info: Option<JexlInfo>, msg: &str, cause: Option<JexlException>) -> Self {
        Self::build(ExceptionKind::Jexl, None, info, Some(JString::from(msg)), cause)
    }

    /// A raw Java throwable (not a JexlException subclass in Java).
    pub fn java(class: &str, message: Option<String>) -> Self {
        Self::build(ExceptionKind::Java { class: class.to_string() }, None, None, message.map(JString::from), None)
    }

    /// A raw Java throwable with a cause.
    pub fn java_with_cause(class: &str, message: Option<String>, cause: Option<JexlException>) -> Self {
        JexlException { kind: ExceptionKind::Java { class: class.to_string() }, mark: None, info: None, detail: message.map(JString::from), cause: cause.map(Box::new) }
    }

    // port of: JexlException.merge
    fn merge(info: Option<JexlInfo>, line: i32, column: i32) -> Option<JexlInfo> {
        if line < 0 {
            return info;
        }
        match info {
            None => Some(JexlInfo::new(Some(String::new()), line, column)),
            Some(i) => Some(JexlInfo::new(i.get_name().map(str::to_string), line, column)),
        }
    }

    // port of: JexlException.Tokenization(JexlInfo, TokenMgrException)
    pub fn tokenization(info: Option<JexlInfo>, line: i32, column: i32, after: JString) -> Self {
        Self::build(ExceptionKind::Tokenization, None, Self::merge(info, line, column), Some(after), None)
    }

    // port of: JexlException.Parsing(JexlInfo, ParseException)
    pub fn parsing_merged(info: Option<JexlInfo>, line: i32, column: i32, after: JString) -> Self {
        Self::build(ExceptionKind::Parsing, None, Self::merge(info, line, column), Some(after), None)
    }

    // port of: JexlException.Parsing(JexlInfo, String)
    pub fn parsing(info: Option<JexlInfo>, msg: &JString) -> Self {
        Self::build(ExceptionKind::Parsing, None, info, Some(msg.clone()), None)
    }

    // port of: JexlException.Ambiguous(JexlInfo, JexlInfo, String)
    pub fn ambiguous(begin: Option<JexlInfo>, end: Option<JexlInfo>, expr: &JString) -> Self {
        Self::build(ExceptionKind::Ambiguous { recover: end }, None, begin, Some(expr.clone()), None)
    }

    // port of: JexlException.StackOverflow(JexlInfo, String, Throwable)
    pub fn stack_overflow(info: Option<JexlInfo>, name: &str, cause: Option<JexlException>) -> Self {
        Self::build(ExceptionKind::StackOverflow, None, info, Some(JString::from(name)), cause)
    }

    // port of: JexlException.Assignment(JexlInfo, String)
    pub fn assignment(info: Option<JexlInfo>, expr: &JString) -> Self {
        Self::build(ExceptionKind::Assignment, None, info, Some(expr.clone()), None)
    }

    // port of: JexlException.Feature(JexlInfo, int, String)
    pub fn feature(info: Option<JexlInfo>, feature: i32, expr: Option<&JString>) -> Self {
        Self::build(ExceptionKind::Feature { code: feature }, None, info, Some(expr.cloned().unwrap_or_else(JString::empty)), None)
    }

    // port of: JexlException.Variable(JexlNode, String, VariableIssue)
    pub fn variable(node: Option<NodeHandle>, var: &JString, issue: VariableIssue) -> Self {
        let info = Self::node_info(&node);
        Self::build(ExceptionKind::Variable { issue }, node, info, Some(var.clone()), None)
    }

    // port of: JexlException.Property(JexlNode, String, boolean, Throwable)
    pub fn property(node: Option<NodeHandle>, pty: &str, undef: bool, cause: Option<JexlException>) -> Self {
        let info = Self::node_info(&node);
        Self::build(ExceptionKind::Property { undefined: undef }, node, info, Some(JString::from(pty)), cause)
    }

    // port of: JexlException.Method(JexlNode, String, Object[])
    pub fn method(node: Option<NodeHandle>, name: &str, args: Option<&[Value]>, cause: Option<JexlException>) -> Self {
        let info = Self::node_info(&node);
        Self::build(ExceptionKind::Method, node, info, Some(JString::from(Self::method_signature(name, args))), cause)
    }

    // port of: JexlException.Method(JexlInfo, String, Object[], Throwable)
    pub fn method_info(info: Option<JexlInfo>, name: &str, args: Option<&[Value]>, cause: Option<JexlException>) -> Self {
        Self::build(ExceptionKind::Method, None, info, Some(JString::from(Self::method_signature(name, args))), cause)
    }

    // port of: JexlException.Operator(JexlNode, String, Throwable)
    pub fn operator(node: Option<NodeHandle>, symbol: &str, cause: Option<JexlException>) -> Self {
        let info = Self::node_info(&node);
        Self::build(ExceptionKind::Operator, node, info, Some(JString::from(symbol)), cause)
    }

    // port of: JexlException.Annotation(JexlNode, String, Throwable)
    pub fn annotation(node: Option<NodeHandle>, name: &str, cause: Option<JexlException>) -> Self {
        let info = Self::node_info(&node);
        Self::build(ExceptionKind::Annotation, node, info, Some(JString::from(name)), cause)
    }

    // port of: JexlException.Return(JexlNode, String, Object)
    pub fn return_(node: Option<NodeHandle>, msg: &str, value: Value) -> Self {
        let info = Self::node_info(&node);
        Self::build(ExceptionKind::Return { value }, node, info, Some(JString::from(msg)), None)
    }

    // port of: JexlException.Cancel(JexlNode)
    pub fn cancel(node: Option<NodeHandle>) -> Self {
        let info = Self::node_info(&node);
        Self::build(ExceptionKind::Cancel, node, info, Some(JString::from("execution cancelled")), None)
    }

    // port of: JexlException.Break(JexlNode)
    pub fn break_(node: Option<NodeHandle>) -> Self {
        let info = Self::node_info(&node);
        Self::build(ExceptionKind::Break, node, info, Some(JString::from("break loop")), None)
    }

    // port of: JexlException.Continue(JexlNode)
    pub fn continue_(node: Option<NodeHandle>) -> Self {
        let info = Self::node_info(&node);
        Self::build(ExceptionKind::Continue, node, info, Some(JString::from("continue loop")), None)
    }

    // port of: JexlException.tryFailed(InvocationTargetException)
    pub fn try_failed(cause: JexlException) -> Self {
        if !matches!(cause.kind, ExceptionKind::Java { .. }) {
            return cause;
        }
        JexlException { kind: ExceptionKind::TryFailed, mark: None, info: None, detail: Some(JString::from("tryFailed")), cause: Some(Box::new(cause)) }
    }

    // port of: JxltEngine.Exception(JexlInfo, String, Throwable)
    pub fn jxlt(info: Option<JexlInfo>, msg: &str, cause: Option<JexlException>) -> Self {
        Self::build(ExceptionKind::Jxlt, None, info, Some(JString::from(msg)), cause)
    }

    // port of: JexlException.methodSignature
    pub fn method_signature(name: &str, args: Option<&[Value]>) -> String {
        match args {
            Some(args) if !args.is_empty() => {
                let names: Vec<String> = args
                    .iter()
                    .map(|a| if a.is_null() { "Object".to_string() } else { a.simple_name() })
                    .collect();
                format!("{}({})", name, names.join(", "))
            }
            _ => name.to_string(),
        }
    }

    pub fn kind(&self) -> &ExceptionKind {
        &self.kind
    }

    /// The node this exception was raised at (JexlException.mark).
    pub fn mark(&self) -> Option<&NodeHandle> {
        self.mark.as_ref()
    }

    /// The raw info (JexlException.info()).
    pub fn info(&self) -> Option<&JexlInfo> {
        self.info.as_ref()
    }

    // port of: JexlException.getInfo — the info, with the Debugger-rendered detail when a node is marked
    pub fn get_info(&self) -> Option<JexlInfo> {
        match (&self.info, &self.mark) {
            (Some(info), Some(mark)) => match crate::internal::debugger::Debugger::detail_of(mark) {
                Some(d) => Some(info.with_detail(d)),
                None => Some(info.clone()),
            },
            (info, _) => info.clone(),
        }
    }

    pub fn get_cause(&self) -> Option<&JexlException> {
        self.cause.as_deref()
    }

    /// Replaces the cause (Throwable.initCause-like, used when wrapping).
    pub(crate) fn set_cause(&mut self, cause: Option<JexlException>) {
        self.cause = cause.map(Box::new);
    }

    // port of: JexlException.getDetail
    pub fn get_detail(&self) -> Option<&JString> {
        self.detail.as_ref()
    }

    /// The Java class name, relative to org.apache.commons.jexl3 for JEXL classes
    /// ("JexlException$Variable", "JxltEngine$Exception", "java.lang.ArithmeticException").
    pub fn class_name(&self) -> String {
        match &self.kind {
            ExceptionKind::Jexl => "JexlException".into(),
            ExceptionKind::Tokenization => "JexlException$Tokenization".into(),
            ExceptionKind::Parsing => "JexlException$Parsing".into(),
            ExceptionKind::Ambiguous { .. } => "JexlException$Ambiguous".into(),
            ExceptionKind::StackOverflow => "JexlException$StackOverflow".into(),
            ExceptionKind::Assignment => "JexlException$Assignment".into(),
            ExceptionKind::Feature { .. } => "JexlException$Feature".into(),
            ExceptionKind::Variable { .. } => "JexlException$Variable".into(),
            ExceptionKind::Property { .. } => "JexlException$Property".into(),
            ExceptionKind::Method => "JexlException$Method".into(),
            ExceptionKind::Operator => "JexlException$Operator".into(),
            ExceptionKind::Annotation => "JexlException$Annotation".into(),
            ExceptionKind::Return { .. } => "JexlException$Return".into(),
            ExceptionKind::Cancel => "JexlException$Cancel".into(),
            ExceptionKind::Break => "JexlException$Break".into(),
            ExceptionKind::Continue => "JexlException$Continue".into(),
            ExceptionKind::TryFailed => "JexlException$TryFailed".into(),
            ExceptionKind::Jxlt => "JxltEngine$Exception".into(),
            ExceptionKind::Java { class } => class.clone(),
        }
    }

    /// Whether this is a JEXL exception (as opposed to a raw Java throwable).
    pub fn is_jexl(&self) -> bool {
        !matches!(self.kind, ExceptionKind::Java { .. })
    }

    // port of: JexlException.parserError
    fn parser_error(&self, prefix: &str, expr: &JString) -> JString {
        let units = expr.units();
        let length = units.len();
        if length < MAX_EXCHARLOC {
            return JStringBuilder::new().str(prefix).str(" error in '").jstr(expr).str("'").build();
        }
        let me = (MAX_EXCHARLOC / 2) as i32;
        let column = self.info.as_ref().map(|i| i.get_column()).unwrap_or(0);
        let mut begin = column - me;
        if begin < 0 || (length as i32) < me {
            begin = 0;
        } else if begin > length as i32 {
            begin = me;
        }
        let mut end = begin as usize + MAX_EXCHARLOC;
        if end > length {
            end = length;
        }
        JStringBuilder::new()
            .str(prefix)
            .str(" error near '... ")
            .units(&units[begin as usize..end])
            .str(" ...'")
            .build()
    }

    // port of: JexlException.detailedMessage (and subclass overrides)
    fn detailed_message(&self) -> JString {
        let empty = JString::empty();
        let detail = self.detail.as_ref().unwrap_or(&empty);
        let wrap = |a: &str, b: &str| JStringBuilder::new().str(a).jstr(detail).str(b).build();
        match &self.kind {
            ExceptionKind::Jexl => wrap("JEXL error : ", ""),
            ExceptionKind::Tokenization => self.parser_error("tokenization", detail),
            ExceptionKind::Parsing => self.parser_error("parsing", detail),
            ExceptionKind::Ambiguous { .. } => self.parser_error("ambiguous statement", detail),
            ExceptionKind::StackOverflow => wrap("stack overflow ", ""),
            ExceptionKind::Assignment => self.parser_error("assignment", detail),
            ExceptionKind::Feature { code } => self.parser_error(JexlFeatures::stringify(*code), detail),
            ExceptionKind::Variable { issue } => issue.message(detail),
            ExceptionKind::Property { undefined } => {
                wrap(if *undefined { "undefined property '" } else { "null value property '" }, "'")
            }
            ExceptionKind::Method => wrap("unsolvable function/method '", "'"),
            ExceptionKind::Operator => wrap("error calling operator '", "'"),
            ExceptionKind::Annotation => wrap("error processing annotation '", "'"),
            ExceptionKind::Return { .. } => wrap("return error : ", ""),
            ExceptionKind::Cancel => wrap("cancel error : ", ""),
            ExceptionKind::Break => wrap("break error : ", ""),
            ExceptionKind::Continue => wrap("continue error : ", ""),
            ExceptionKind::TryFailed => wrap("tryfailed error : ", ""),
            ExceptionKind::Jxlt => wrap("exception error : ", ""),
            ExceptionKind::Java { .. } => detail.clone(),
        }
    }

    // port of: JexlException.getMessage (Throwable.getMessage for raw Java throwables)
    pub fn get_message(&self) -> Option<JString> {
        if let ExceptionKind::Java { .. } = self.kind {
            return self.detail.clone();
        }
        let mut msg = JStringBuilder::new();
        match &self.info {
            Some(i) => msg.jstr(&i.to_jstring()),
            None => msg.str("?:"),
        };
        msg.str(" ").jstr(&self.detailed_message());
        if let Some(c) = &self.cause {
            if matches!(&c.kind, ExceptionKind::Java { class } if class == "JexlArithmetic$NullOperand") {
                msg.str(" caused by null operand");
            }
        }
        Some(msg.build())
    }

    /// get_message() with Java's "null" rendering for a null message.
    pub fn message(&self) -> String {
        self.get_message().map(|m| m.to_rust()).unwrap_or_else(|| "null".to_string())
    }

    // port of: JexlException.Ambiguous.tryCleanSource
    pub fn try_clean_source(&self, src: &str) -> String {
        if let ExceptionKind::Ambiguous { recover: Some(end) } = &self.kind {
            if let Some(ji) = &self.info {
                return Self::slice_source(src, ji.get_line(), ji.get_column(), end.get_line(), end.get_column());
            }
        }
        src.to_string()
    }

    // port of: JexlException.sliceSource
    pub fn slice_source(src: &str, froml: i32, fromc: i32, tol: i32, toc: i32) -> String {
        let mut buffer = String::new();
        for (cl, line) in java_lines(src).iter().enumerate() {
            let cl = cl as i32 + 1;
            if cl < froml || cl > tol {
                buffer.push_str(line);
                buffer.push('\n');
            } else {
                let u: Vec<u16> = line.encode_utf16().collect();
                if cl == froml {
                    let e = ((fromc - 1).max(0) as usize).min(u.len());
                    buffer.push_str(&String::from_utf16_lossy(&u[..e]));
                }
                if cl == tol {
                    let b = ((toc + 1).max(0) as usize).min(u.len());
                    buffer.push_str(&String::from_utf16_lossy(&u[b..]));
                }
            }
        }
        buffer
    }

    /// The value carried by a Return exception.
    pub fn return_value(&self) -> Option<&Value> {
        match &self.kind {
            ExceptionKind::Return { value } => Some(value),
            _ => None,
        }
    }
}

/// BufferedReader.readLine splitting: \n, \r and \r\n terminate lines; no trailing empty line.
pub(crate) fn java_lines(src: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let b = src.as_bytes();
    let mut start = 0;
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'\n' => {
                out.push(&src[start..i]);
                start = i + 1;
            }
            b'\r' => {
                out.push(&src[start..i]);
                if i + 1 < b.len() && b[i + 1] == b'\n' {
                    i += 1;
                }
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    if start < b.len() {
        out.push(&src[start..]);
    }
    out
}

impl fmt::Debug for JexlException {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.class_name(), self.message())?;
        if let Some(c) = &self.cause {
            write!(f, " <- {:?}", c)?;
        }
        Ok(())
    }
}

impl fmt::Display for JexlException {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message())
    }
}

impl std::error::Error for JexlException {}
