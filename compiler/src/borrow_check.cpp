#include "kardashev/borrow_check.hpp"

#include <unordered_map>
#include <utility>
#include <vector>

namespace kardashev {
namespace {

// Per-binding ownership state. Phase 2.4a needs only two states; Moved is
// terminal until scope exit. Phase 2.4c will add Borrowed{n_shared,
// has_mut} on top of this.
enum class OwnState { Owned, Moved };

struct BindingState {
    OwnState state = OwnState::Owned;
    bool isMoveTyped = true; // false for Copy bindings (i64 / bool / Unit)
    std::size_t moveLine = 0;
    std::size_t moveCol = 0;
};

// Classify a Type as Copy. Phase 2.4a: only primitives are Copy; every
// Struct / Enum is Move. Phase 2.4b can introduce a `Copy` marker trait;
// for now the rule is mechanical and matches Rust's default for compound
// types (struct/enum aren't Copy unless declared).
bool isCopyType(const TypePtr& t) {
    if (!t) return true; // play safe on missing type info
    TypePtr r = resolve(t);
    switch (r->kind) {
    case TypeKind::Int:
    case TypeKind::Bool:
    case TypeKind::Unit:
        return true;
    case TypeKind::Var:
        // Generic Vars (in a generic body) are conservatively Move so the
        // most-restrictive case is enforced. Codegen monomorphizes; the
        // borrow-check is run once on the generic body and therefore must
        // assume the worst case.
        return false;
    case TypeKind::Function:
    case TypeKind::Struct:
    case TypeKind::Enum:
        return false;
    }
    return false;
}

class BorrowChecker {
public:
    BorrowChecker(const ast::Program& program, const TypeCheckResult& tc)
        : program_(program), tc_(tc) {}

    BorrowCheckResult run() {
        for (const auto& fn : program_.functions) checkFn(fn);
        // Impl-method bodies live inside `program_.impls`. Phase 2.4a
        // treats them identically to top-level fns — same scoping, same
        // move semantics on `self`.
        for (const auto& impl : program_.impls) {
            for (const auto& fn : impl.methods) checkFn(fn);
        }
        return std::move(result_);
    }

private:
    const ast::Program& program_;
    const TypeCheckResult& tc_;
    BorrowCheckResult result_;
    // Active scope stack of binding tables. Each LetStmt / fn param /
    // match-arm pattern pushes onto the innermost scope.
    std::vector<std::unordered_map<std::string, BindingState>> scopes_;

    void error(std::string msg, std::size_t line, std::size_t col) {
        result_.errors.push_back({std::move(msg), line, col});
    }

    BindingState* lookup(const std::string& name) {
        for (auto it = scopes_.rbegin(); it != scopes_.rend(); ++it) {
            auto found = it->find(name);
            if (found != it->end()) return &found->second;
        }
        return nullptr;
    }

    void bind(const std::string& name, const TypePtr& ty) {
        if (scopes_.empty()) scopes_.push_back({});
        BindingState st;
        st.state = OwnState::Owned;
        st.isMoveTyped = !isCopyType(ty);
        scopes_.back()[name] = st;
    }

    TypePtr typeOf(const ast::Expr& e) {
        auto it = tc_.exprTypes.find(&e);
        if (it == tc_.exprTypes.end()) return nullptr;
        return it->second;
    }

    void checkFn(const ast::FnDecl& fn) {
        scopes_.clear();
        scopes_.push_back({});
        for (const auto& p : fn.params) {
            auto it = tc_.exprTypes.end();
            (void)it;
            // Parameter types come from the AST TypeRef; the typechecker
            // resolves these, but we don't get an exprTypes entry for the
            // bare param. Reconstruct Copy/Move classification by name.
            bind(p.name, paramTypeFromAst(p.type));
        }
        if (fn.body) {
            consume(*fn.body); // body's tail value is "returned" / consumed
        }
        scopes_.clear();
    }

    // Fallback type lookup for fn parameters: we don't have a direct
    // exprTypes entry. Build a minimal TypePtr from the TypeRef name —
    // enough to drive Copy/Move classification (we only care which
    // category, not the full structure).
    TypePtr paramTypeFromAst(const ast::TypeRef& tr) {
        if (tr.name == "i64") return makeInt();
        if (tr.name == "bool") return makeBool();
        // Anything else (struct, enum, generic param, &T-once-we-have-it) is
        // conservatively Move-typed. A schema-lookup against tc_.structs /
        // tc_.enums would refine this, but for classification we only need
        // "is this Copy?" — the answer for compounds is `no`.
        auto t = std::make_shared<Type>();
        t->kind = TypeKind::Struct; // any non-Copy kind picks the Move side
        t->structName = tr.name;
        return t;
    }

    // Top-level expression visit. Treat every IdentExpr we visit through
    // `consume` as a whole-use of its binding; FieldExpr / MethodCallExpr /
    // CallExpr / etc dispatch by AST shape so the right slots get
    // consumed.
    void consume(const ast::Expr& e) {
        if (auto* id = dynamic_cast<const ast::IdentExpr*>(&e)) {
            consumeIdent(*id);
            return;
        }
        if (dynamic_cast<const ast::IntLitExpr*>(&e)) return;
        if (auto* bin = dynamic_cast<const ast::BinaryExpr*>(&e)) {
            // Binary ops are i64/bool only (enforced by typechecker), and
            // both sides are Copy. Operands still need to be walked so any
            // nested moves register.
            consume(*bin->lhs);
            consume(*bin->rhs);
            return;
        }
        if (auto* call = dynamic_cast<const ast::CallExpr*>(&e)) {
            for (const auto& a : call->args) consume(*a);
            return;
        }
        if (auto* mc = dynamic_cast<const ast::MethodCallExpr*>(&e)) {
            // self is consumed by value (Phase 2.4a; `&self` arrives in
            // Phase 2.4b alongside reference types).
            consume(*mc->receiver);
            for (const auto& a : mc->args) consume(*a);
            return;
        }
        if (auto* ie = dynamic_cast<const ast::IfExpr*>(&e)) {
            consume(*ie->cond);
            consume(*ie->thenBranch);
            consume(*ie->elseBranch);
            return;
        }
        if (auto* block = dynamic_cast<const ast::BlockExpr*>(&e)) {
            consumeBlock(*block);
            return;
        }
        if (auto* sl = dynamic_cast<const ast::StructLitExpr*>(&e)) {
            for (const auto& [_n, v] : sl->fields) consume(*v);
            return;
        }
        if (auto* fe = dynamic_cast<const ast::FieldExpr*>(&e)) {
            // FieldExpr's .object is NOT consumed as a whole — we just
            // read one field. Phase 2.4a fields are Copy (verified by the
            // typechecker via the field's resolved type); a Move-typed
            // field would need a partial-move state, deferred to 2.4b.
            //
            // We still need to detect: was the OWNER already moved? If so
            // its fields are not accessible either.
            if (auto* ido = dynamic_cast<const ast::IdentExpr*>(fe->object.get())) {
                checkRead(*ido);
            } else {
                // Field access on a non-Ident receiver (e.g. f(x).field).
                // Evaluate the inner expression normally — temporary result
                // doesn't outlive this statement and there's no binding to
                // mark moved.
                consume(*fe->object);
            }
            return;
        }
        if (auto* me = dynamic_cast<const ast::MatchExpr*>(&e)) {
            // Scrutinee is consumed; arm bodies are then checked with their
            // own pattern bindings in fresh sub-scopes.
            consume(*me->scrutinee);
            for (const auto& arm : me->arms) {
                scopes_.push_back({});
                bindPattern(*arm.pattern, typeOf(*me->scrutinee));
                consume(*arm.body);
                scopes_.pop_back();
            }
            return;
        }
        if (auto* te = dynamic_cast<const ast::TryExpr*>(&e)) {
            // `expr?` consumes the operand (either unwraps Ok or early-
            // returns Err).
            consume(*te->operand);
            return;
        }
    }

    void consumeIdent(const ast::IdentExpr& id) {
        BindingState* st = lookup(id.name);
        if (!st) return; // unknown / fn name / variant ctor — not a binding
        if (!st->isMoveTyped) return;
        if (st->state == OwnState::Moved) {
            error("use of moved value `" + id.name + "` (moved at " +
                      std::to_string(st->moveLine) + ":" +
                      std::to_string(st->moveCol) + ")",
                  id.line, id.column);
            return;
        }
        st->state = OwnState::Moved;
        st->moveLine = id.line;
        st->moveCol = id.column;
    }

    // Pure "read" check — used for FieldExpr's owner-Ident slot. Reports
    // use-after-move but doesn't transition the binding's state, because
    // a field read isn't a whole-value move.
    void checkRead(const ast::IdentExpr& id) {
        BindingState* st = lookup(id.name);
        if (!st) return;
        if (!st->isMoveTyped) return;
        if (st->state == OwnState::Moved) {
            error("field access on moved value `" + id.name + "` (moved at " +
                      std::to_string(st->moveLine) + ":" +
                      std::to_string(st->moveCol) + ")",
                  id.line, id.column);
        }
    }

    void consumeBlock(const ast::BlockExpr& block) {
        scopes_.push_back({});
        for (const auto& stmt : block.stmts) {
            if (auto* let = dynamic_cast<const ast::LetStmt*>(stmt.get())) {
                consume(*let->value);
                bind(let->name, typeOf(*let->value));
                continue;
            }
            if (auto* ret = dynamic_cast<const ast::ReturnStmt*>(stmt.get())) {
                if (ret->value) consume(*ret->value);
                continue;
            }
            if (auto* es = dynamic_cast<const ast::ExprStmt*>(stmt.get())) {
                consume(*es->expr);
                continue;
            }
        }
        if (block.tail) consume(*block.tail);
        scopes_.pop_back();
    }

    // Walk a pattern and bind any introduced names. Phase 2.4a: every
    // VarPat binding is treated as owning its sub-value, classified by the
    // pattern's expected type. Sub-pattern types come from the variant's
    // payload list (looked up via tc_.variantIndex).
    void bindPattern(const ast::Pattern& pat, const TypePtr& expected) {
        if (auto* vp = dynamic_cast<const ast::VarPat*>(&pat)) {
            // Unit-ctor pattern (rewritten as VarPat by the parser) doesn't
            // introduce a binding — `None` consumes nothing.
            auto vit = tc_.variantIndex.find(vp->name);
            if (vit == tc_.variantIndex.end()) {
                bind(vp->name, expected);
            }
            return;
        }
        if (auto* cp = dynamic_cast<const ast::CtorPat*>(&pat)) {
            // Locate the variant's payload types via the resolved enum
            // schema. If the expected type's enumVariants list has the
            // ctor, use payload types from THAT instance — they're
            // already substituted for generic typeArgs.
            TypePtr re = expected ? resolve(expected) : nullptr;
            std::vector<TypePtr> payloadTypes;
            if (re && re->kind == TypeKind::Enum) {
                for (const auto& v : re->enumVariants) {
                    if (v.name == cp->ctorName) {
                        payloadTypes = v.payloadTypes;
                        break;
                    }
                }
            }
            for (std::size_t i = 0; i < cp->subpatterns.size(); ++i) {
                TypePtr sub = (i < payloadTypes.size())
                                  ? payloadTypes[i]
                                  : TypePtr{};
                bindPattern(*cp->subpatterns[i], sub);
            }
            return;
        }
        // LitIntPat / WildPat introduce no bindings.
    }
};

} // namespace

BorrowCheckResult borrow_check(const ast::Program& program,
                                const TypeCheckResult& tc) {
    BorrowChecker bc(program, tc);
    return bc.run();
}

} // namespace kardashev
