//SPEC: §18.2 an inferred integer literal binding is `i64`, not any narrower type
//ERR: E0110
pub fn main() void {
    var x = 5; // inferred i64
    var y: i32 = 1;
    print(x + y); // i64 + i32 — must be rejected; proves x is not i32
}
