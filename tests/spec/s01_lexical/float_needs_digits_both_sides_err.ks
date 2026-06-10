//SPEC: §1/§38 a float literal requires digits on BOTH sides of the dot — `.5` and `5.` are not float literals and fail to parse
//ERR: E0200
// The parser reports E0200 here, but modules::resolve wraps every root-file
// (see tests/spec-quarantine for the exact-code pin of this discrepancy).
pub fn main() void {
    var f: f64 = .5;
    var g: f64 = 5.;
    print(f + g);
}
