//SPEC: §11.2 a value that is neither `T` nor `?T` at a `?T` coercion site is an E0110 mismatch
//ERR: E0110

pub fn main() void {
    var x: ?i64 = true;   // bool is not i64 (and not ?i64)
}
