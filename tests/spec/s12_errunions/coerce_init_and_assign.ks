//SPEC: §12.2 `T -> !T` (success) and `error.X -> !T` (failure) coerce at a `!T` initializer and assignment
//OUT: 42
//OUT: -1
//OUT: 9
//OUT: -1

pub fn main() void {
    var x: !i64 = 6 * 7;       // T -> !T at the initializer
    print(x catch 0 - 1);      // 42
    x = error.Flip;            // error.X -> !T at an assignment
    print(x catch 0 - 1);      // -1
    x = 5 + 4;                 // T -> !T at an assignment (back to success)
    print(x catch 0 - 1);      // 9
    var y: !i64 = error.Flip;  // error.X -> !T at the initializer
    print(y catch 0 - 1);      // -1
}
