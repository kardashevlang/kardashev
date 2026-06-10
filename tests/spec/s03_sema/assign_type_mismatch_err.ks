//SPEC: §3 an assignment's RHS type must match the target's declared type
//ERR: E0110
pub fn main() void {
    var x: i32 = 1;
    var y: i64 = 2;
    x = y;
    print(x);
}
