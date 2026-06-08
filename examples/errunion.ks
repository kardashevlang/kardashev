// errunion.ks — error unions (v0.115): `!T`, `error.X`, `try`, `catch`.
//
// A function returning `!T` either yields a `T` or an error value (`error.X`).
// `try e` unwraps `e` or propagates its error out of the (also `!T`) caller.
// `e catch default` unwraps `e` or falls back to `default` on error.

fn checked_div(a: i32, b: i32) !i32 {
    if (b == 0) {
        return error.DivByZero;
    }
    return a / b;
}

// Propagates errors from the inner divisions with `try`.
fn average3(a: i32, b: i32, c: i32, n: i32) !i32 {
    var total: i32 = a + b + c;
    var avg: i32 = try checked_div(total, n);
    return avg;
}

pub fn main() i32 {
    print(checked_div(20, 4) catch 0 - 1);   // 5
    print(checked_div(20, 0) catch 0 - 1);   // -1  (error.DivByZero)
    print(average3(3, 6, 9, 3) catch 0 - 1); // 6
    print(average3(3, 6, 9, 0) catch 0 - 1); // -1  (try propagates DivByZero)
    return 0;
}

test "checked division" {
    expect((checked_div(20, 4) catch 0) == 5);
    expect((checked_div(1, 0) catch 999) == 999);
    expect((average3(3, 6, 9, 3) catch 0) == 6);
    expect((average3(1, 1, 1, 0) catch 42) == 42);
}
