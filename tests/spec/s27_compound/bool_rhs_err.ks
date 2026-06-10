//SPEC: §27.2 the rhs must be an integer — a `bool` rhs is the usual binop type error
//ERR: E0110

pub fn main() void {
    var x: i64 = 1;
    var b: bool = true;
    x += b;
}
