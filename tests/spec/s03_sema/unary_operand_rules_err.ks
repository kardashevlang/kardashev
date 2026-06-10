//SPEC: §3 unary `-` requires a signed integer; `!` requires a `bool`
//ERR: E0110
// Both violations live in one file because they are halves of the same SPEC
// bullet; each one independently produces E0110.
fn neg_unsigned() void {
    var x: u32 = 5;
    print(-x); // `-` on an unsigned integer
}
fn not_int() void {
    if (!3) { // `!` on an integer
        print(1);
    }
}
pub fn main() void {
    neg_unsigned();
    not_int();
}
