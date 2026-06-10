//SPEC: §3 constant evaluation applies the same type rules — `int + bool` is a comptime type error
//ERR: E0132
const A: i64 = 1 + true;
pub fn main() void {
    print(A);
}
