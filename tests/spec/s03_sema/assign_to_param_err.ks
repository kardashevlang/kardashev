//SPEC: §3 parameters are immutable bindings — assigning to a parameter is rejected
//ERR: E0110
fn bump(n: i64) i64 {
    n = n + 1;
    return n;
}
pub fn main() void {
    print(bump(1));
}
