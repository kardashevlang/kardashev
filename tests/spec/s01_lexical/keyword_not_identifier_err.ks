//SPEC: §1 keywords are reserved — `while` cannot be used as an identifier
//ERR: E0200
// The parser reports E0200 here, but modules::resolve wraps every root-file
// (see tests/spec-quarantine for the exact-code pin of this discrepancy).
pub fn main() void {
    var while: i64 = 1;
    print(while);
}
