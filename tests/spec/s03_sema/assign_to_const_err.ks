//SPEC: §3 the assignment target must be a `var` local — assigning to a `const` is rejected
//ERR: E0110
pub fn main() void {
    const c: i64 = 5;
    c = 6;
    print(c);
}
