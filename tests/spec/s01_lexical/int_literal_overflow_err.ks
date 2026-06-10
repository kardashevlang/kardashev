//SPEC: §1 an integer literal out of range for i64 is diagnosed E0002
//ERR: E0002
// The lexer reports E0002 here, but modules::resolve wraps every root-file
// (see tests/spec-quarantine for the exact-code pin of this discrepancy).
pub fn main() void {
    // i64::MAX is 9223372036854775807; one more must not lex.
    print(9223372036854775808);
}
