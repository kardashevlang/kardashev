// Unit tests for kardashev::pattern_match (Maranget exhaustiveness).
//
// Tests are constructed by hand-building AST patterns + types — no
// dependency on the parser or typechecker so this slice is verifiable
// in isolation.

#include "kardashev/pattern_match.hpp"

#include <cassert>
#include <iostream>
#include <memory>
#include <string>
#include <unordered_map>
#include <utility>
#include <vector>

using kardashev::EnumVariantType;
using kardashev::TypePtr;
using kardashev::makeEnum;
using kardashev::makeInt;
using kardashev::makeStruct;
using kardashev::pattern_match::checkExhaustiveness;
using kardashev::pattern_match::Witness;

namespace ast = kardashev::ast;

namespace {

// --- AST pattern builders ---

ast::PatternPtr wild() {
    return std::make_unique<ast::WildPat>();
}

ast::PatternPtr lit(std::int64_t v) {
    auto p = std::make_unique<ast::LitIntPat>();
    p->value = v;
    return p;
}

ast::PatternPtr var(std::string name) {
    auto p = std::make_unique<ast::VarPat>();
    p->name = std::move(name);
    return p;
}

ast::PatternPtr ctor(std::string name, std::vector<ast::PatternPtr> subs = {}) {
    auto p = std::make_unique<ast::CtorPat>();
    p->ctorName = std::move(name);
    p->subpatterns = std::move(subs);
    return p;
}

ast::MatchArm arm(ast::PatternPtr p) {
    ast::MatchArm a;
    a.pattern = std::move(p);
    // body is intentionally null — exhaustiveness checker doesn't need it.
    return a;
}

// --- Variant-index helpers ---

using VariantIndex =
    std::unordered_map<std::string, std::pair<std::string, unsigned>>;
using EnumMap = std::unordered_map<std::string, TypePtr>;

// Build Maybe = Some(i64) | None.
TypePtr maybeType() {
    std::vector<EnumVariantType> vs;
    EnumVariantType some;
    some.name = "Some";
    some.payloadTypes = {makeInt()};
    vs.push_back(some);
    EnumVariantType none;
    none.name = "None";
    vs.push_back(none);
    return makeEnum("Maybe", vs);
}

// Build Color = Red | Green | Blue (all unit variants).
TypePtr colorType() {
    std::vector<EnumVariantType> vs;
    EnumVariantType r; r.name = "Red"; vs.push_back(r);
    EnumVariantType g; g.name = "Green"; vs.push_back(g);
    EnumVariantType b; b.name = "Blue"; vs.push_back(b);
    return makeEnum("Color", vs);
}

struct OuterInner {
    TypePtr inner;
    TypePtr outer;
};

// Inner = A | B; Outer = O(Inner) | N.
OuterInner outerInnerTypes() {
    std::vector<EnumVariantType> innerVs;
    EnumVariantType a; a.name = "A"; innerVs.push_back(a);
    EnumVariantType b; b.name = "B"; innerVs.push_back(b);
    TypePtr inner = makeEnum("Inner", innerVs);

    std::vector<EnumVariantType> outerVs;
    EnumVariantType o; o.name = "O"; o.payloadTypes = {inner}; outerVs.push_back(o);
    EnumVariantType n; n.name = "N"; outerVs.push_back(n);
    TypePtr outer = makeEnum("Outer", outerVs);

    return {inner, outer};
}

void expectExhaustive(const TypePtr& ty,
                      std::vector<ast::MatchArm> arms,
                      const EnumMap& enums,
                      const VariantIndex& vidx,
                      const char* label) {
    auto w = checkExhaustiveness(ty, arms, enums, vidx);
    if (w) {
        std::cerr << "[" << label << "] expected exhaustive, got witness: "
                  << w->text << '\n';
        std::abort();
    }
}

void expectNonExhaustive(const TypePtr& ty,
                         std::vector<ast::MatchArm> arms,
                         const EnumMap& enums,
                         const VariantIndex& vidx,
                         const std::string& expectedWitness,
                         const char* label) {
    auto w = checkExhaustiveness(ty, arms, enums, vidx);
    if (!w) {
        std::cerr << "[" << label
                  << "] expected non-exhaustive, but result was exhaustive\n";
        std::abort();
    }
    if (w->text != expectedWitness) {
        std::cerr << "[" << label << "] witness mismatch: expected "
                  << expectedWitness << ", got " << w->text << '\n';
        std::abort();
    }
}

// --- Tests ---

void test_empty_arms_int() {
    std::vector<ast::MatchArm> arms;
    expectNonExhaustive(makeInt(), std::move(arms), {}, {}, "_",
                        "empty_arms_int");
}

void test_empty_arms_enum() {
    auto ty = maybeType();
    VariantIndex vidx;
    vidx["Some"] = {"Maybe", 0};
    vidx["None"] = {"Maybe", 1};
    EnumMap em; em["Maybe"] = ty;
    std::vector<ast::MatchArm> arms;
    // With zero arms, witness is the first missing variant — Some(_).
    expectNonExhaustive(ty, std::move(arms), em, vidx, "Some(_)",
                        "empty_arms_enum");
}

void test_single_wildcard_int() {
    std::vector<ast::MatchArm> arms;
    arms.push_back(arm(wild()));
    expectExhaustive(makeInt(), std::move(arms), {}, {}, "single_wildcard_int");
}

void test_single_varpat_int() {
    std::vector<ast::MatchArm> arms;
    arms.push_back(arm(var("x"))); // not in variantIndex → wildcard
    expectExhaustive(makeInt(), std::move(arms), {}, {}, "single_varpat_int");
}

void test_int_literals_no_wildcard() {
    std::vector<ast::MatchArm> arms;
    arms.push_back(arm(lit(0)));
    arms.push_back(arm(lit(1)));
    arms.push_back(arm(lit(2)));
    expectNonExhaustive(makeInt(), std::move(arms), {}, {}, "_",
                        "int_literals_no_wildcard");
}

void test_maybe_some_and_none() {
    auto ty = maybeType();
    VariantIndex vidx;
    vidx["Some"] = {"Maybe", 0};
    vidx["None"] = {"Maybe", 1};
    EnumMap em; em["Maybe"] = ty;
    std::vector<ast::MatchArm> arms;
    {
        std::vector<ast::PatternPtr> subs; subs.push_back(wild());
        arms.push_back(arm(ctor("Some", std::move(subs))));
    }
    arms.push_back(arm(ctor("None")));
    expectExhaustive(ty, std::move(arms), em, vidx, "maybe_some_and_none");
}

void test_maybe_only_some() {
    auto ty = maybeType();
    VariantIndex vidx;
    vidx["Some"] = {"Maybe", 0};
    vidx["None"] = {"Maybe", 1};
    EnumMap em; em["Maybe"] = ty;
    std::vector<ast::MatchArm> arms;
    {
        std::vector<ast::PatternPtr> subs; subs.push_back(var("x"));
        arms.push_back(arm(ctor("Some", std::move(subs))));
    }
    expectNonExhaustive(ty, std::move(arms), em, vidx, "None",
                        "maybe_only_some");
}

void test_maybe_only_none() {
    auto ty = maybeType();
    VariantIndex vidx;
    vidx["Some"] = {"Maybe", 0};
    vidx["None"] = {"Maybe", 1};
    EnumMap em; em["Maybe"] = ty;
    std::vector<ast::MatchArm> arms;
    arms.push_back(arm(ctor("None")));
    expectNonExhaustive(ty, std::move(arms), em, vidx, "Some(_)",
                        "maybe_only_none");
}

void test_color_all_variants() {
    auto ty = colorType();
    VariantIndex vidx;
    vidx["Red"] = {"Color", 0};
    vidx["Green"] = {"Color", 1};
    vidx["Blue"] = {"Color", 2};
    EnumMap em; em["Color"] = ty;
    std::vector<ast::MatchArm> arms;
    arms.push_back(arm(ctor("Red")));
    arms.push_back(arm(ctor("Green")));
    arms.push_back(arm(ctor("Blue")));
    expectExhaustive(ty, std::move(arms), em, vidx, "color_all_variants");
}

void test_color_missing_blue() {
    auto ty = colorType();
    VariantIndex vidx;
    vidx["Red"] = {"Color", 0};
    vidx["Green"] = {"Color", 1};
    vidx["Blue"] = {"Color", 2};
    EnumMap em; em["Color"] = ty;
    std::vector<ast::MatchArm> arms;
    arms.push_back(arm(ctor("Red")));
    arms.push_back(arm(ctor("Green")));
    expectNonExhaustive(ty, std::move(arms), em, vidx, "Blue",
                        "color_missing_blue");
}

void test_nested_outer_missing_inner_b() {
    auto oi = outerInnerTypes();
    VariantIndex vidx;
    vidx["A"] = {"Inner", 0};
    vidx["B"] = {"Inner", 1};
    vidx["O"] = {"Outer", 0};
    vidx["N"] = {"Outer", 1};
    EnumMap em; em["Inner"] = oi.inner; em["Outer"] = oi.outer;
    std::vector<ast::MatchArm> arms;
    {
        std::vector<ast::PatternPtr> subs;
        subs.push_back(ctor("A"));
        arms.push_back(arm(ctor("O", std::move(subs))));
    }
    arms.push_back(arm(ctor("N")));
    expectNonExhaustive(oi.outer, std::move(arms), em, vidx, "O(B)",
                        "nested_outer_missing_inner_b");
}

void test_nested_outer_inner_both_present() {
    auto oi = outerInnerTypes();
    VariantIndex vidx;
    vidx["A"] = {"Inner", 0};
    vidx["B"] = {"Inner", 1};
    vidx["O"] = {"Outer", 0};
    vidx["N"] = {"Outer", 1};
    EnumMap em; em["Inner"] = oi.inner; em["Outer"] = oi.outer;
    std::vector<ast::MatchArm> arms;
    {
        std::vector<ast::PatternPtr> subs;
        subs.push_back(ctor("A"));
        arms.push_back(arm(ctor("O", std::move(subs))));
    }
    {
        std::vector<ast::PatternPtr> subs;
        subs.push_back(ctor("B"));
        arms.push_back(arm(ctor("O", std::move(subs))));
    }
    arms.push_back(arm(ctor("N")));
    expectExhaustive(oi.outer, std::move(arms), em, vidx,
                     "nested_outer_inner_both_present");
}

void test_nested_outer_with_wildcard_inner() {
    auto oi = outerInnerTypes();
    VariantIndex vidx;
    vidx["A"] = {"Inner", 0};
    vidx["B"] = {"Inner", 1};
    vidx["O"] = {"Outer", 0};
    vidx["N"] = {"Outer", 1};
    EnumMap em; em["Inner"] = oi.inner; em["Outer"] = oi.outer;
    std::vector<ast::MatchArm> arms;
    {
        std::vector<ast::PatternPtr> subs;
        subs.push_back(wild());
        arms.push_back(arm(ctor("O", std::move(subs))));
    }
    arms.push_back(arm(ctor("N")));
    expectExhaustive(oi.outer, std::move(arms), em, vidx,
                     "nested_outer_with_wildcard_inner");
}

void test_struct_with_varpat_exhaustive() {
    // Struct has no constructor signature in V1 — any wildcard/var arm
    // covers all values.
    auto ty = makeStruct("Point",
                         {{"x", makeInt()}, {"y", makeInt()}});
    std::vector<ast::MatchArm> arms;
    arms.push_back(arm(var("p")));
    expectExhaustive(ty, std::move(arms), {}, {},
                     "struct_with_varpat_exhaustive");
}

void test_varpat_unit_ctor_rewrite() {
    // Parser produces VarPat("Red") etc.; the variantIndex rewrites these
    // to unit-Ctor — exhaustiveness must therefore treat all-VarPat as
    // exhaustive when each name corresponds to a variant.
    auto ty = colorType();
    VariantIndex vidx;
    vidx["Red"] = {"Color", 0};
    vidx["Green"] = {"Color", 1};
    vidx["Blue"] = {"Color", 2};
    EnumMap em; em["Color"] = ty;
    std::vector<ast::MatchArm> arms;
    arms.push_back(arm(var("Red")));
    arms.push_back(arm(var("Green")));
    arms.push_back(arm(var("Blue")));
    expectExhaustive(ty, std::move(arms), em, vidx,
                     "varpat_unit_ctor_rewrite");
}

void test_int_literals_with_wildcard() {
    // Sanity check: literals plus a wildcard arm is exhaustive.
    std::vector<ast::MatchArm> arms;
    arms.push_back(arm(lit(0)));
    arms.push_back(arm(lit(1)));
    arms.push_back(arm(wild()));
    expectExhaustive(makeInt(), std::move(arms), {}, {},
                     "int_literals_with_wildcard");
}

void test_maybe_some_with_inner_int_lit_no_wildcard() {
    // match m { Some(0) => ..., None => ... } — non-exhaustive
    // because Some(1), Some(2), ... aren't covered. Witness Some(_).
    auto ty = maybeType();
    VariantIndex vidx;
    vidx["Some"] = {"Maybe", 0};
    vidx["None"] = {"Maybe", 1};
    EnumMap em; em["Maybe"] = ty;
    std::vector<ast::MatchArm> arms;
    {
        std::vector<ast::PatternPtr> subs; subs.push_back(lit(0));
        arms.push_back(arm(ctor("Some", std::move(subs))));
    }
    arms.push_back(arm(ctor("None")));
    expectNonExhaustive(ty, std::move(arms), em, vidx, "Some(_)",
                        "maybe_some_with_inner_int_lit_no_wildcard");
}

} // namespace

int main() {
    test_empty_arms_int();
    test_empty_arms_enum();
    test_single_wildcard_int();
    test_single_varpat_int();
    test_int_literals_no_wildcard();
    test_maybe_some_and_none();
    test_maybe_only_some();
    test_maybe_only_none();
    test_color_all_variants();
    test_color_missing_blue();
    test_nested_outer_missing_inner_b();
    test_nested_outer_inner_both_present();
    test_nested_outer_with_wildcard_inner();
    test_struct_with_varpat_exhaustive();
    test_varpat_unit_ctor_rewrite();
    test_int_literals_with_wildcard();
    test_maybe_some_with_inner_int_lit_no_wildcard();
    std::cout << "All pattern_match tests passed (17 cases)\n";
    return 0;
}
