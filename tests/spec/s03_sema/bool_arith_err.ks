//SPEC: §3 `bool` is not an integer: arithmetic on a `bool` operand is rejected
//ERR: E0110
pub fn main() void {
    print(1 + true);
}
