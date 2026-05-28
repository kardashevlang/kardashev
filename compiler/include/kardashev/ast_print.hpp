// AST pretty-printer for kardashev — the backing library for `kard fmt`.
//
// Walks a parsed `ast::Program` and renders *canonical* kardashev source.
// The printer is deliberately decoupled from LLVM (lexer + parser only) so
// the `kardfmt` tool stays small and fast.
//
// Canonical style:
//   - 4-space indentation, no tabs.
//   - `{ }` blocks open on the same line as their header (`fn f() -> T {`).
//   - One statement per line, each terminated where the grammar requires.
//   - Binary operators surrounded by single spaces (`a + b`).
//   - Effect rows printed as ` ! { a, b }` (empty rows are omitted).
//   - `match` arms one per line, 4-space indented, trailing comma each.
//   - Top-level items separated by a single blank line, emitted in a stable
//     order: mods, structs, enums, traits, impls, functions.
//
// Idempotency contract: format(parse(format(parse(src)))) == format(parse(src)).
// Because the printer only consumes the AST (never raw token trivia), the
// output is a fixed function of the parse tree, so a second round trip is a
// byte-for-byte fixed point.

#pragma once

#include <string>

#include "kardashev/ast.hpp"

namespace kardashev {

// Render a whole program to canonical source. The returned string always
// ends with a single trailing newline (POSIX text-file convention) unless
// the program is completely empty, in which case it is "".
std::string formatProgram(const ast::Program& program);

} // namespace kardashev
