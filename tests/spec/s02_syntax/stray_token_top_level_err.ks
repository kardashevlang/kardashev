//SPEC: §2 module := item* — a stray token at top level (not fn/const/test) is a parse error
//ERR: E0200
// The parser reports E0200 here, but modules::resolve wraps every root-file
// (see tests/spec-quarantine for the exact-code pin of this discrepancy).
bogus

pub fn main() void {
    print(1);
}
