//SPEC: §3 `return e` is invalid in a `void` fn; `return;` is invalid in a value-returning fn
//ERR: E0110
// Both violations are halves of the same SPEC bullet; each independently
// produces E0110.
fn v() void {
    return 5;
}
fn g() i64 {
    return;
}
pub fn main() void {
    v();
    print(g());
}
