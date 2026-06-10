//SPEC: §3 `and` requires `bool` operands
//ERR: E0110
pub fn main() void {
    if (1 and true) {
        print(1);
    }
}
