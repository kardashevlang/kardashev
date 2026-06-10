//SPEC: §23.1 there is no string concatenation operator — `+` on `[]u8` operands is E0110 (std `str_concat` is the library route)
//ERR: E0110

pub fn main() void {
    var s: []u8 = "ab" + "cd";
    print(s);
}
