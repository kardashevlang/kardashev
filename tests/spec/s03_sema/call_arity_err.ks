//SPEC: §3 a call must pass exactly as many arguments as the callee declares
//ERR: E0110
fn add(a: i64, b: i64) i64 {
    return a + b;
}
pub fn main() void {
    print(add(1));
}
