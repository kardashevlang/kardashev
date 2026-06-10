//SPEC: §27.2 a compound assignment requires an assignable place — a `const` binding is rejected exactly as for `=`
//ERR: E0110

pub fn main() void {
    const c: i64 = 4;
    c += 1;
}
