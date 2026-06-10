//SPEC: §15.1 `p.*` requires a pointer operand — dereferencing a non-pointer is E0230
//ERR: E0230

pub fn main() void {
    var x: i64 = 1;
    var y: i64 = x.*; // `x` is an i64, not a `*T`
    print(y);
}
