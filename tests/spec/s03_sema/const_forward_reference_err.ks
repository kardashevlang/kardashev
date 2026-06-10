//SPEC: §3 a `const` may only reference consts defined earlier in source order
//ERR: E0131
const A: i64 = B + 1;
const B: i64 = 2;
pub fn main() void {
    print(A);
}
