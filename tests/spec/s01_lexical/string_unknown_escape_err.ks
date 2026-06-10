//SPEC: §1 only \n \t \\ \" are string escapes — an unknown escape like \q is diagnosed E0001
//ERR: E0001
// The lexer reports E0001 here, but modules::resolve wraps every root-file
// (see tests/spec-quarantine for the exact-code pin of this discrepancy).
pub fn main() void {
    var s: []u8 = "a\qb";
    print(s);
}
