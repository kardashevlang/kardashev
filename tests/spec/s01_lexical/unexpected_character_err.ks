//SPEC: §1 a character outside the language's alphabet is diagnosed E0001
//ERR: E0001
// The lexer reports E0001 here, but modules::resolve wraps every root-file
// (see tests/spec-quarantine for the exact-code pin of this discrepancy).
pub fn main() void {
    print(1) # ;
}
