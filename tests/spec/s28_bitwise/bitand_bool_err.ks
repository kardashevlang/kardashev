//SPEC: §28.2 `&` requires integer operands — `bool & bool` is rejected (use the `and` keyword)
//ERR: E0110

pub fn main() void {
    var t: bool = true;
    var f: bool = false;
    var r: bool = t & f;
}
