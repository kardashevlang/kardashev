//SPEC: §3 `print` accepts integers, `f64`, and `[]u8` — a bool argument is rejected
//ERR: E0110

pub fn main() void {
    print(true);
}
