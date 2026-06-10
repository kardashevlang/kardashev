//SPEC: §28.2 a shift's operands must be the SAME integer type — `u64 << u8` is rejected
//ERR: E0110

pub fn main() void {
    var x: u64 = 1;
    var n: u8 = 3;
    print(x << n);
}
