//SPEC: §15.1 `&place` requires an lvalue — taking the address of a temporary expression is E0231
//ERR: E0231

pub fn main() void {
    var x: i64 = 1;
    var p: *i64 = &(x + 1); // `x + 1` is an rvalue, not a place
    p.* = 2;
}
