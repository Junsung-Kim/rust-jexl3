// port of: org.apache.commons.jexl3.internal.Operators
use crate::jexl_arithmetic::{ArithError, JexlArithmetic};
use crate::jexl_exception::JexlException;
use crate::jexl_operator::JexlOperator;
use crate::parser::jexl_node::NodeRef;
use crate::value::Value;

use super::interpreter::{Interpreter, TRY_FAILED};

/// Helper resolving operators; a custom arithmetic can overload any of them through the uberspect.
pub struct Operators;

impl Operators {
    /// port of: Operators.tryOverload — `TRY_FAILED` when no overload applies
    pub fn try_overload(it: &Interpreter, node: NodeRef<'_>, operator: JexlOperator, args: &[Value]) -> Result<Value, JexlException> {
        if !it.uberspect.overloads(operator) {
            return Ok(TRY_FAILED.clone());
        }
        match it.uberspect.get_operator(operator, args) {
            None => Ok(TRY_FAILED.clone()),
            Some(vm) => match vm.invoke(&Value::Null, args) {
                Ok(v) => Ok(v),
                Err(e) => it.operator_error(node, operator, Some(e)),
            },
        }
    }

    /// port of: Operators.tryAssignOverload — the self-* operators fall back to their base operator
    pub fn try_assign_overload(
        it: &Interpreter,
        node: NodeRef<'_>,
        operator: JexlOperator,
        args: &[Value],
    ) -> Result<Value, JexlException> {
        if args.len() as i32 != operator.get_arity() {
            return Ok(TRY_FAILED.clone());
        }
        let result = Self::try_overload(it, node, operator, args)?;
        if !is_try_failed(&result) {
            return Ok(result);
        }
        let base = operator.get_base_operator().expect("must be called with a side-effect operator");
        if it.uberspect.overloads(base) {
            if let Some(vm) = it.uberspect.get_operator(base, args) {
                match vm.invoke(&Value::Null, args) {
                    Ok(v) if !is_try_failed(&v) => return Ok(v),
                    Ok(_) => {}
                    Err(e) => {
                        it.operator_error(node, base, Some(e))?;
                    }
                }
            }
        }
        let a = &it.arithmetic;
        let r = match operator {
            JexlOperator::SelfAdd => a.add(&args[0], &args[1]),
            JexlOperator::SelfSubtract => a.subtract(&args[0], &args[1]),
            JexlOperator::SelfMultiply => a.multiply(&args[0], &args[1]),
            JexlOperator::SelfDivide => a.divide(&args[0], &args[1]),
            JexlOperator::SelfMod => a.modulo(&args[0], &args[1]),
            JexlOperator::SelfAnd => a.and(&args[0], &args[1]),
            JexlOperator::SelfOr => a.or(&args[0], &args[1]),
            JexlOperator::SelfXor => a.xor(&args[0], &args[1]),
            other => {
                return Err(JexlException::java(
                    "java.lang.UnsupportedOperationException",
                    Some(other.get_operator_symbol().to_string()),
                ))
            }
        };
        match r {
            Ok(v) => Ok(v),
            Err(e) => {
                it.operator_error(node, base, Some(it.arith_exception(node, e)))?;
                Ok(TRY_FAILED.clone())
            }
        }
    }

    /// port of: Operators.startsWith
    pub fn starts_with(it: &Interpreter, node: NodeRef<'_>, operator: &str, left: &Value, right: &Value) -> Result<bool, JexlException> {
        let result = Self::try_overload(it, node, JexlOperator::StartsWith, &[left.clone(), right.clone()])?;
        if let Value::Boolean(b) = result {
            return Ok(b);
        }
        match it.arithmetic.starts_with(left, right) {
            Ok(Some(b)) => return Ok(b),
            Ok(None) => {}
            Err(e) => return Err(Self::op_error(it, node, operator, e)),
        }
        if let Some(b) = Self::boolean_method(it, node, left, "startsWith", right, operator)? {
            return Ok(b);
        }
        match it.arithmetic.equals(left, right) {
            Ok(b) => Ok(b),
            Err(e) => Err(Self::op_error(it, node, operator, e)),
        }
    }

    /// port of: Operators.endsWith
    pub fn ends_with(it: &Interpreter, node: NodeRef<'_>, operator: &str, left: &Value, right: &Value) -> Result<bool, JexlException> {
        let result = Self::try_overload(it, node, JexlOperator::EndsWith, &[left.clone(), right.clone()])?;
        if let Value::Boolean(b) = result {
            return Ok(b);
        }
        match it.arithmetic.ends_with(left, right) {
            Ok(Some(b)) => return Ok(b),
            Ok(None) => {}
            Err(e) => return Err(Self::op_error(it, node, operator, e)),
        }
        if let Some(b) = Self::boolean_method(it, node, left, "endsWith", right, operator)? {
            return Ok(b);
        }
        match it.arithmetic.equals(left, right) {
            Ok(b) => Ok(b),
            Err(e) => Err(Self::op_error(it, node, operator, e)),
        }
    }

    /// port of: Operators.contains — note the interpreter passes (right, left) for `=~`
    pub fn contains(it: &Interpreter, node: NodeRef<'_>, op: &str, left: &Value, right: &Value) -> Result<bool, JexlException> {
        let result = Self::try_overload(it, node, JexlOperator::Contains, &[left.clone(), right.clone()])?;
        if let Value::Boolean(b) = result {
            return Ok(b);
        }
        match it.arithmetic.contains(left, right) {
            Ok(Some(b)) => return Ok(b),
            Ok(None) => {}
            Err(e) => return Err(Self::op_error(it, node, op, e)),
        }
        if let Some(b) = Self::boolean_method(it, node, left, "contains", right, op)? {
            return Ok(b);
        }
        match it.arithmetic.equals(left, right) {
            Ok(b) => Ok(b),
            Err(e) => Err(Self::op_error(it, node, op, e)),
        }
    }

    /// The `uberspect.getMethod(left, name, {right})` fallback the three operators above share.
    fn boolean_method(
        it: &Interpreter,
        node: NodeRef<'_>,
        left: &Value,
        name: &str,
        right: &Value,
        operator: &str,
    ) -> Result<Option<bool>, JexlException> {
        let mut argv = [right.clone()];
        if let Some(vm) = it.uberspect.get_method(left, name, &argv) {
            if returns_boolean(&vm) {
                return match vm.invoke(left, &argv) {
                    Ok(Value::Boolean(b)) => Ok(Some(b)),
                    Ok(_) => Ok(None),
                    Err(e) => Err(JexlException::new(Some(it.handle(node)), &format!("{} error", operator), Some(e))),
                };
            }
        }
        if it.arithmetic.narrow_arguments(&mut argv) {
            if let Some(vm) = it.uberspect.get_method(left, name, &argv) {
                if returns_boolean(&vm) {
                    return match vm.invoke(left, &argv) {
                        Ok(Value::Boolean(b)) => Ok(Some(b)),
                        Ok(_) => Ok(None),
                        Err(e) => Err(JexlException::new(Some(it.handle(node)), &format!("{} error", operator), Some(e))),
                    };
                }
            }
        }
        Ok(None)
    }

    /// Java catches only `ArithmeticException` around these operators; anything else propagates.
    fn op_error(it: &Interpreter, node: NodeRef<'_>, operator: &str, e: ArithError) -> JexlException {
        let cause = it.arith_exception(node, e.clone());
        match e {
            ArithError::NullOperand | ArithError::Arithmetic(_) => {
                JexlException::new(Some(it.handle(node)), &format!("{} error", operator), Some(cause))
            }
            _ => cause,
        }
    }

    /// port of: Operators.empty
    pub fn empty(it: &Interpreter, node: NodeRef<'_>, object: &Value) -> Result<Value, JexlException> {
        if object.is_null() {
            return Ok(Value::Boolean(true));
        }
        let result = Self::try_overload(it, node, JexlOperator::Empty, &[object.clone()])?;
        if !is_try_failed(&result) {
            return Ok(result);
        }
        let mut result = match it.arithmetic.is_empty(object, None) {
            Ok(Some(b)) => Value::Boolean(b),
            Ok(None) => Value::Null,
            Err(e) => return Err(it.arith_exception(node, e)),
        };
        if result.is_null() {
            result = Value::Boolean(false);
            if let Some(vm) = it.uberspect.get_method(object, "isEmpty", &[]) {
                if returns_boolean(&vm) {
                    match vm.invoke(object, &[]) {
                        Ok(v) => result = v,
                        Err(e) => {
                            it.operator_error(node, JexlOperator::Empty, Some(e))?;
                        }
                    }
                }
            }
        }
        Ok(Value::Boolean(match result {
            Value::Boolean(b) => b,
            _ => true,
        }))
    }

    /// port of: Operators.size
    pub fn size(it: &Interpreter, node: NodeRef<'_>, object: &Value) -> Result<Value, JexlException> {
        if object.is_null() {
            return Ok(Value::Integer(0));
        }
        let result = Self::try_overload(it, node, JexlOperator::Size, &[object.clone()])?;
        if !is_try_failed(&result) {
            return Ok(result);
        }
        let mut result = match it.arithmetic.size(object, None) {
            Ok(Some(i)) => Value::Integer(i),
            Ok(None) => Value::Null,
            Err(e) => return Err(it.arith_exception(node, e)),
        };
        if result.is_null() {
            if let Some(vm) = it.uberspect.get_method(object, "size", &[]) {
                if returns_integer(&vm) {
                    match vm.invoke(object, &[]) {
                        Ok(v) => result = v,
                        Err(e) => {
                            it.operator_error(node, JexlOperator::Size, Some(e))?;
                        }
                    }
                }
            }
        }
        Ok(Value::Integer(match &result {
            v if v.is_number() => it.arithmetic.to_integer(v).unwrap_or(0),
            _ => 0,
        }))
    }
}

/// Whether the value is the `JexlEngine.TRY_FAILED` sentinel.
pub fn is_try_failed(v: &Value) -> bool {
    v.as_host::<super::interpreter::TryFailed>().is_some()
}

// port of: Operators.returnsBoolean
fn returns_boolean(vm: &std::sync::Arc<dyn crate::introspection::JexlMethod>) -> bool {
    matches!(vm.return_type().as_deref(), Some("boolean") | Some("java.lang.Boolean"))
}

// port of: Operators.returnsInteger
fn returns_integer(vm: &std::sync::Arc<dyn crate::introspection::JexlMethod>) -> bool {
    matches!(vm.return_type().as_deref(), Some("int") | Some("java.lang.Integer"))
}
