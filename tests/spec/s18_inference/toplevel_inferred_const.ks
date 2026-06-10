//SPEC: §18.2 a top-level inferred `const` adopts its comptime value's type (`i64`/`bool`)
//OUT: 40
//OUT: 42
const ANSWER = 6 * 7;        // inferred i64, folded to 42
const SMALL = ANSWER < 100;  // inferred bool, folded to true
pub fn main() void {
    var x: i64 = ANSWER - 2; // mixes with annotated i64 — proves i64
    print(x);
    if (SMALL) { // an `if` condition only accepts bool — proves bool
        print(ANSWER);
    }
}
