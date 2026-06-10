//SPEC: §3 the returned value's type must match the function return type
//ERR: E0110
fn f() i32 {
    var y: i64 = 2;
    return y;
}
pub fn main() void {
    print(f());
}
