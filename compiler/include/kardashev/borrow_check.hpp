// Phase 2.4a: move-semantics checker for kardashev.
//
// Runs after the typechecker (consumes its TypeCheckResult) and walks every
// function body to enforce affine ownership on Move-typed bindings: each
// non-Copy value can be consumed at most once.
//
// Classification (Phase 2.4a):
//   - Copy:  i64, bool. Multiple uses are fine; the binding never moves.
//   - Move:  Struct, Enum (and any future compound type). The binding starts
//            in state Owned; a "whole use" transitions it to Moved; any
//            subsequent read while Moved is an error.
//
// What counts as a "whole use" of an IdentExpr that resolves to a binding:
//   - Right-hand side of `let x = ident_y;`           — moves y into x
//   - Argument of a `CallExpr` / `MethodCallExpr`      — moves into callee
//   - Scrutinee of a `MatchExpr`                       — match consumes it
//   - Field-init value of a `StructLitExpr`            — moves into field
//   - Argument of a constructor call (`Some(y)`)       — moves into payload
//   - Return-value expression                          — moves out of fn
//   - Tail expression of a block (when its value is consumed by an outer use)
//   - Operand of `?` (TryExpr)                         — consumed
//
// NOT a whole use:
//   - The `.object` slot of a `FieldExpr` (`y.field`)  — Phase 2.4a treats
//     this as borrowing y briefly to read one field. Fields are Copy in Phase
//     2.4a (Move-typed fields require partial moves, which arrive in 2.4b).
//   - The `.receiver` slot of a `MethodCallExpr` when `self` is by-value —
//     IS a whole use (the impl takes ownership of self).
//
// Phase 2.4a is intentionally conservative: a single linear walk per function
// body, no NLL refinement, no references. Phase 2.4b adds `&T`; Phase 2.4c
// adds `&mut T` and the non-lexical region machinery.

#pragma once

#include <cstddef>
#include <string>
#include <vector>

#include "kardashev/ast.hpp"
#include "kardashev/typecheck.hpp"

namespace kardashev {

struct BorrowError {
    std::string message;
    std::size_t line = 1;
    std::size_t column = 1;
};

struct BorrowCheckResult {
    std::vector<BorrowError> errors;
    bool ok() const { return errors.empty(); }
};

// Run the move-semantics pass. The `tc` argument provides per-Expr type
// information (used to classify Copy vs Move). Returns errors as a list;
// callers (kardc / tests) decide how to surface them. A typecheck failure
// upstream is the caller's responsibility — this pass assumes
// `tc.errors.empty()`.
BorrowCheckResult borrow_check(const ast::Program& program,
                                const TypeCheckResult& tc);

} // namespace kardashev
