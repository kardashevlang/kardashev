//SPEC: §17.1 the composite form `!T` substitutes through a generic return type
//OUT: 14
//OUT: -1
//OUT: 10309278

// A generic returning `!T`: the success path coerces `T` into the substituted
// error union, the failure path coerces `error.X`, and the caller observes
// both via `catch`.
fn safe_div(comptime T: type, a: T, b: T) !T {
    if (b == 0) {
        return error.DivByZero;
    }
    return a / b;
}

pub fn main() void {
    print(safe_div(i32, 100, 7) catch 0 - 1);            // 100 / 7 = 14
    print(safe_div(i32, 5, 0) catch 0 - 1);              // error -> -1
    print(safe_div(i64, 1000000007, 97) catch 0 - 1);    // = 10309278 (rem 41)
}
