//SPEC: §18.2 an inferred binding adopts `bool` from a comparison or bool-returning call
//OUT: 1
fn is_even(n: i64) bool {
    return n % 2 == 0;
}
pub fn main() void {
    var b = is_even(10); // inferred bool, true
    var c = 3 < 2;       // inferred bool, false
    if (b and !c) {      // `and`/`!` only accept bool — proves both inferences
        print(1);
    } else {
        print(0);
    }
}
