// port of: org.apache.commons.jexl3.JexlOperator

/// The operators that JexlArithmetic implements and that a custom arithmetic can overload.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum JexlOperator {
    Add,
    Subtract,
    Multiply,
    Divide,
    Mod,
    And,
    Or,
    Xor,
    Eq,
    Lt,
    Lte,
    Gt,
    Gte,
    Contains,
    StartsWith,
    EndsWith,
    Not,
    Complement,
    Negate,
    Positivize,
    Empty,
    Size,
    SelfAdd,
    SelfSubtract,
    SelfMultiply,
    SelfDivide,
    SelfMod,
    SelfAnd,
    SelfOr,
    SelfXor,
    Assign,
    PropertyGet,
    PropertySet,
    ArrayGet,
    ArraySet,
    ForEach,
}

use JexlOperator::*;

impl JexlOperator {
    // port of: JexlOperator.getOperatorSymbol
    pub fn get_operator_symbol(&self) -> &'static str {
        match self {
            Add | Positivize => "+",
            Subtract | Negate => "-",
            Multiply => "*",
            Divide => "/",
            Mod => "%",
            And => "&",
            Or => "|",
            Xor => "^",
            Eq => "==",
            Lt => "<",
            Lte => "<=",
            Gt => ">",
            Gte => ">=",
            Contains => "=~",
            StartsWith => "=^",
            EndsWith => "=$",
            Not => "!",
            Complement => "~",
            Empty => "empty",
            Size => "size",
            SelfAdd => "+=",
            SelfSubtract => "-=",
            SelfMultiply => "*=",
            SelfDivide => "/=",
            SelfMod => "%=",
            SelfAnd => "&=",
            SelfOr => "|=",
            SelfXor => "^=",
            Assign => "=",
            PropertyGet => ".",
            PropertySet => ".=",
            ArrayGet => "[]",
            ArraySet => "[]=",
            ForEach => "for(...)",
        }
    }

    // port of: JexlOperator.getMethodName
    pub fn get_method_name(&self) -> Option<&'static str> {
        Some(match self {
            Add => "add",
            Subtract => "subtract",
            Multiply => "multiply",
            Divide => "divide",
            Mod => "mod",
            And => "and",
            Or => "or",
            Xor => "xor",
            Eq => "equals",
            Lt => "lessThan",
            Lte => "lessThanOrEqual",
            Gt => "greaterThan",
            Gte => "greaterThanOrEqual",
            Contains => "contains",
            StartsWith => "startsWith",
            EndsWith => "endsWith",
            Not => "not",
            Complement => "complement",
            Negate => "negate",
            Positivize => "positivize",
            Empty => "empty",
            Size => "size",
            SelfAdd => "selfAdd",
            SelfSubtract => "selfSubtract",
            SelfMultiply => "selfMultiply",
            SelfDivide => "selfDivide",
            SelfMod => "selfMod",
            SelfAnd => "selfAnd",
            SelfOr => "selfOr",
            SelfXor => "selfXor",
            Assign => return None,
            PropertyGet => "propertyGet",
            PropertySet => "propertySet",
            ArrayGet => "arrayGet",
            ArraySet => "arraySet",
            ForEach => "forEach",
        })
    }

    // port of: JexlOperator.getArity
    pub fn get_arity(&self) -> i32 {
        match self {
            Not | Complement | Negate | Positivize | Empty | Size | ForEach => 1,
            PropertySet | ArraySet => 3,
            _ => 2,
        }
    }

    // port of: JexlOperator.getBaseOperator
    pub fn get_base_operator(&self) -> Option<JexlOperator> {
        Some(match self {
            SelfAdd => Add,
            SelfSubtract => Subtract,
            SelfMultiply => Multiply,
            SelfDivide => Divide,
            SelfMod => Mod,
            SelfAnd => And,
            SelfOr => Or,
            SelfXor => Xor,
            _ => return None,
        })
    }
}
