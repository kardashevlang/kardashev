//SPEC: §11.1 a `null` with no expected optional type at its position is E0180
//ERR: E0180

// The annotated target is a plain i64, not an optional, so `null` has no
// expected `?T` to adopt.
pub fn main() void {
    var x: i64 = null;
    print(x);
}
