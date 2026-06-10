//SPEC: §2 a type is written as a name (type := IDENT ...) — an integer literal in type position is a parse error
//ERR: E0200
// The parser reports E0200 here, but modules::resolve wraps every root-file
// (see tests/spec-quarantine for the exact-code pin of this discrepancy).
pub fn main() void {
    var x: 123 = 1;
    print(x);
}
