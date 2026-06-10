//SPEC: §3 a top-level `const` initializer must be const-evaluable — a function call is not
//ERR: E0130
// The call sits inside a larger expression: a *bare* call initializer is
// claimed by the §25.2 type-alias-instantiation form (E0311) instead.
fn f() i64 {
    return 1;
}
const A: i64 = 1 + f();
pub fn main() void {
    print(A);
}
