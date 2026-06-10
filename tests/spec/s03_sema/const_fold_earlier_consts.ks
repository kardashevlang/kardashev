//SPEC: §3 top-level `const` initializers fold at compile time over earlier consts; `comptime e` folds in a body
//OUT: 36
//OUT: 77
const A: i64 = 6;
const B: i64 = A * 7;  // 42 — references the earlier const A
const C: i64 = B - A;  // 36 — references both earlier consts
pub fn main() void {
    print(C);
    var x: i64 = comptime (B + C - 1); // 42 + 36 - 1 = 77, folded by const_eval
    print(x);
}
