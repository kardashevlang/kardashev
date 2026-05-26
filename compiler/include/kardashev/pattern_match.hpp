// Pattern-match exhaustiveness check for kardashev V1.
//
// Implements Maranget's usefulness algorithm (from "Compiling Pattern
// Matching to Good Decision Trees", 2008) to detect non-exhaustive
// `match` expressions and synthesize a missing-pattern witness.
//
// This library is intentionally typecheck-independent: callers pass in
// the scrutinee type plus the program's enum table and variant index so
// the typecheck library can later depend on this one (and not the other
// way around).

#pragma once

#include <optional>
#include <string>
#include <unordered_map>
#include <utility>
#include <vector>

#include "kardashev/ast.hpp"
#include "kardashev/types.hpp"

namespace kardashev::pattern_match {

struct Witness {
    // Pretty-printed example pattern not covered by the arms. e.g. "None",
    // "Some(_)", "O(B)", or "_" if the scrutinee type is infinite.
    std::string text;
};

// Returns nullopt if `arms` exhaustively cover all values of
// `scrutineeType`; otherwise returns a Witness with a missing-pattern
// example.
//
// `enums` and `variantIndex` come from TypeCheckResult (passed in to
// avoid a circular library dep on typecheck).
std::optional<Witness> checkExhaustiveness(
    const TypePtr& scrutineeType,
    const std::vector<ast::MatchArm>& arms,
    const std::unordered_map<std::string, TypePtr>& enums,
    const std::unordered_map<std::string, std::pair<std::string, unsigned>>&
        variantIndex);

} // namespace kardashev::pattern_match
