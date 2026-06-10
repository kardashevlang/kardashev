//SPEC: §3 defining a `fn` named after a builtin (`print`) is rejected
//ERR: E0101
fn print(x: i64) void {
}
pub fn main() void {
    print(1);
}
