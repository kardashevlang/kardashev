//SPEC: §27.2 the place and rhs must be the SAME integer type — i32 place, i64 rhs is rejected
//ERR: E0110

pub fn main() void {
    var x: i32 = 1;
    var y: i64 = 2;
    x += y;
}
