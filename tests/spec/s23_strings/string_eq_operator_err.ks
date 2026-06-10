//SPEC: §3 comparison operands must be int or bool (f64 since §38) — `==` on `[]u8` strings should be a sema type error (E0110)
//ERR: E0110

pub fn main() void {
    if ("ab" == "ab") { print(1); } else { print(0); }
}
