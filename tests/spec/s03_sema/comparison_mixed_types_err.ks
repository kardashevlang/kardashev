//SPEC: §3 comparisons require both operands the same type
//ERR: E0110
pub fn main() void {
    var a: u8 = 1;
    var b: i64 = 1;
    if (a < b) {
        print(1);
    }
}
