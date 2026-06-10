// Import fixture (§22.1): deliberately UNLEXABLE (\q is no escape, E0001) —
// the importing test pins that sub-file LEX errors wrap into E0294 too.
fn lex_oops() void {
    var s: []u8 = "a\qb";
}
