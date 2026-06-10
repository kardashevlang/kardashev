//SPEC: §28.2 unary `~` requires an integer — `~bool` is rejected (use `!`)
//ERR: E0110

pub fn main() void {
    var t: bool = true;
    var r: bool = ~t;
}
