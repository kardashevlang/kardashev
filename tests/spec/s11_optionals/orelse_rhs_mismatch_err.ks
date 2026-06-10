//SPEC: §11.1 the `orelse` alternative must be the inner `T`; any other type is an E0110 mismatch
//ERR: E0110

pub fn main() void {
    var x: ?i64 = 5;
    var y: i64 = x orelse true;   // alternative is bool, inner type is i64
    print(y);
}
