//! Compile-time constant evaluation, shared by `sema` (to validate `comptime`
//! and top-level `const` initializers) and used to fold them.
//!
//! The evaluator works over [`Expr`] and a map of already-known top-level
//! constants. It mirrors the SPEC §3 type rules: integer arithmetic wraps as
//! `i64`, comparisons and logical operators yield `bool`, and any shape that is
//! not a compile-time constant (a function call), references an unknown const,
//! or mixes incompatible operand types is reported as a diagnostic in the
//! `E013x` family.

use std::collections::HashMap;

use crate::ast::{BinOp, Expr, UnOp};
use crate::diag::Diagnostic;
use crate::span::Span;

/// A compile-time constant value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConstVal {
    Int(i64),
    Bool(bool),
}

/// Evaluate `expr` to a constant, given the already-known top-level constants.
/// Errors if `expr` references something not known at compile time.
///
/// Error codes:
/// - `E0130` — the expression contains a non-constant construct (a call).
/// - `E0131` — the expression references an unknown / not-yet-defined const.
/// - `E0132` — a type error within the constant expression.
pub fn eval(expr: &Expr, consts: &HashMap<String, ConstVal>) -> Result<ConstVal, Diagnostic> {
    match expr {
        Expr::Int { value, .. } => Ok(ConstVal::Int(*value)),
        Expr::Bool { value, .. } => Ok(ConstVal::Bool(*value)),
        Expr::Ident { name, span } => match consts.get(name) {
            Some(v) => Ok(*v),
            None => Err(Diagnostic::error(
                *span,
                "E0131",
                format!("unknown constant `{}` in constant expression", name),
            )),
        },
        Expr::Comptime { expr, .. } => eval(expr, consts),
        Expr::Unary { op, expr: inner, span } => {
            let v = eval(inner, consts)?;
            eval_unary(*op, v, *span)
        }
        Expr::Binary { op, lhs, rhs, span } => {
            let l = eval(lhs, consts)?;
            let r = eval(rhs, consts)?;
            eval_binary(*op, l, r, *span)
        }
        Expr::Call { span, .. } => Err(Diagnostic::error(
            *span,
            "E0130",
            "function calls are not allowed in a constant expression",
        )),
        // Structs are not compile-time constant values in v0.112.
        Expr::StructLit { span, .. } => Err(Diagnostic::error(
            *span,
            "E0130",
            "struct literals are not allowed in a constant expression",
        )),
        Expr::Field { span, .. } => Err(Diagnostic::error(
            *span,
            "E0130",
            "field access is not allowed in a constant expression",
        )),
        Expr::MethodCall { span, .. } => Err(Diagnostic::error(
            *span,
            "E0130",
            "method calls are not allowed in a constant expression",
        )),
        Expr::Null { span } | Expr::Orelse { span, .. } | Expr::Unwrap { span, .. } => {
            Err(Diagnostic::error(
                *span,
                "E0130",
                "optionals are not allowed in a constant expression",
            ))
        }
        Expr::ErrorLit { span, .. } | Expr::Try { span, .. } | Expr::Catch { span, .. } => {
            Err(Diagnostic::error(
                *span,
                "E0130",
                "error unions are not allowed in a constant expression",
            ))
        }
        Expr::EnumLit { span, .. } => Err(Diagnostic::error(
            *span,
            "E0130",
            "enum values are not allowed in a constant expression",
        )),
    }
}

fn eval_unary(op: UnOp, v: ConstVal, span: Span) -> Result<ConstVal, Diagnostic> {
    match op {
        UnOp::Neg => match v {
            ConstVal::Int(n) => Ok(ConstVal::Int(n.wrapping_neg())),
            ConstVal::Bool(_) => Err(Diagnostic::error(
                span,
                "E0132",
                "unary `-` requires an integer operand in a constant expression",
            )),
        },
        UnOp::Not => match v {
            ConstVal::Bool(b) => Ok(ConstVal::Bool(!b)),
            ConstVal::Int(_) => Err(Diagnostic::error(
                span,
                "E0132",
                "unary `!` requires a bool operand in a constant expression",
            )),
        },
    }
}

fn eval_binary(op: BinOp, l: ConstVal, r: ConstVal, span: Span) -> Result<ConstVal, Diagnostic> {
    match op {
        BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Rem => {
            let (a, b) = match (l, r) {
                (ConstVal::Int(a), ConstVal::Int(b)) => (a, b),
                _ => {
                    return Err(Diagnostic::error(
                        span,
                        "E0132",
                        "arithmetic requires integer operands in a constant expression",
                    ))
                }
            };
            let v = match op {
                BinOp::Add => a.wrapping_add(b),
                BinOp::Sub => a.wrapping_sub(b),
                BinOp::Mul => a.wrapping_mul(b),
                BinOp::Div => {
                    if b == 0 {
                        return Err(Diagnostic::error(
                            span,
                            "E0132",
                            "division by zero in a constant expression",
                        ));
                    }
                    a.wrapping_div(b)
                }
                BinOp::Rem => {
                    if b == 0 {
                        return Err(Diagnostic::error(
                            span,
                            "E0132",
                            "remainder by zero in a constant expression",
                        ));
                    }
                    a.wrapping_rem(b)
                }
                _ => unreachable!(),
            };
            Ok(ConstVal::Int(v))
        }
        BinOp::Eq | BinOp::Ne => {
            let eq = match (l, r) {
                (ConstVal::Int(a), ConstVal::Int(b)) => a == b,
                (ConstVal::Bool(a), ConstVal::Bool(b)) => a == b,
                _ => {
                    return Err(Diagnostic::error(
                        span,
                        "E0132",
                        "comparison requires operands of the same type in a constant expression",
                    ))
                }
            };
            Ok(ConstVal::Bool(if op == BinOp::Eq { eq } else { !eq }))
        }
        BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
            let (a, b) = match (l, r) {
                (ConstVal::Int(a), ConstVal::Int(b)) => (a, b),
                (ConstVal::Bool(a), ConstVal::Bool(b)) => (a as i64, b as i64),
                _ => {
                    return Err(Diagnostic::error(
                        span,
                        "E0132",
                        "comparison requires operands of the same type in a constant expression",
                    ))
                }
            };
            let v = match op {
                BinOp::Lt => a < b,
                BinOp::Le => a <= b,
                BinOp::Gt => a > b,
                BinOp::Ge => a >= b,
                _ => unreachable!(),
            };
            Ok(ConstVal::Bool(v))
        }
        BinOp::And | BinOp::Or => {
            let (a, b) = match (l, r) {
                (ConstVal::Bool(a), ConstVal::Bool(b)) => (a, b),
                _ => {
                    return Err(Diagnostic::error(
                        span,
                        "E0132",
                        "logical operators require bool operands in a constant expression",
                    ))
                }
            };
            Ok(ConstVal::Bool(if op == BinOp::And { a && b } else { a || b }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::span::Span;

    fn sp() -> Span {
        Span::DUMMY
    }
    fn int(v: i64) -> Expr {
        Expr::Int { value: v, span: sp() }
    }
    fn bin(op: BinOp, l: Expr, r: Expr) -> Expr {
        Expr::Binary {
            op,
            lhs: Box::new(l),
            rhs: Box::new(r),
            span: sp(),
        }
    }

    #[test]
    fn folds_arithmetic_expr() {
        // (2 + 3) * 4 == 20
        let e = bin(BinOp::Mul, bin(BinOp::Add, int(2), int(3)), int(4));
        let consts = HashMap::new();
        assert_eq!(eval(&e, &consts), Ok(ConstVal::Int(20)));
    }

    #[test]
    fn folds_with_known_const() {
        // BASE + 5 where BASE = 10
        let mut consts = HashMap::new();
        consts.insert("BASE".to_string(), ConstVal::Int(10));
        let e = bin(
            BinOp::Add,
            Expr::Ident {
                name: "BASE".into(),
                span: sp(),
            },
            int(5),
        );
        assert_eq!(eval(&e, &consts), Ok(ConstVal::Int(15)));
    }

    #[test]
    fn unknown_const_errors() {
        let e = Expr::Ident {
            name: "MISSING".into(),
            span: sp(),
        };
        let consts = HashMap::new();
        let err = eval(&e, &consts).unwrap_err();
        assert_eq!(err.code, "E0131");
    }

    #[test]
    fn call_is_not_constant() {
        let e = Expr::Call {
            callee: "f".into(),
            args: vec![],
            span: sp(),
        };
        let consts = HashMap::new();
        assert_eq!(eval(&e, &consts).unwrap_err().code, "E0130");
    }

    #[test]
    fn type_error_in_const_expr() {
        // 1 + true is a type error
        let e = bin(BinOp::Add, int(1), Expr::Bool { value: true, span: sp() });
        let consts = HashMap::new();
        assert_eq!(eval(&e, &consts).unwrap_err().code, "E0132");
    }

    #[test]
    fn comptime_unwraps_inner() {
        let e = Expr::Comptime {
            expr: Box::new(bin(BinOp::Add, int(40), int(2))),
            span: sp(),
        };
        let consts = HashMap::new();
        assert_eq!(eval(&e, &consts), Ok(ConstVal::Int(42)));
    }

    #[test]
    fn division_by_zero_errors() {
        let e = bin(BinOp::Div, int(1), int(0));
        let consts = HashMap::new();
        assert_eq!(eval(&e, &consts).unwrap_err().code, "E0132");
    }
}
