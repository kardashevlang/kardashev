//SPEC: §2 parentheses around `if`/`while` conditions are required (Zig style)
//ERR: E0200
// The parser reports E0200 here, but modules::resolve wraps every root-file
// (see tests/spec-quarantine for the exact-code pin of this discrepancy).
pub fn main() void {
    var x: i64 = 1;
    if x > 0 {
        print(x);
    }
}
