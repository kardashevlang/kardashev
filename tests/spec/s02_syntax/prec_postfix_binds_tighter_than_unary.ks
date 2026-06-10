//SPEC: §2/§28.1 postfix (index, call) binds tighter than prefix unary — -arr[0] negates the ELEMENT, -f(x) negates the RESULT
//OUT: -17
//OUT: -6
fn double(n: i64) i64 {
    return n * 2;
}

pub fn main() void {
    var arr: [2]i64 = [2]i64{ 7, 9 };
    // (-arr[0]) + (~arr[1]) = -7 + -10 = -17.
    print(-arr[0] + ~arr[1]);
    // -(double(3)) = -6.
    print(-double(3));
}
