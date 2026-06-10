//SPEC: §12.1 a `!T` function returns either a payload `T` or an `error.Name` value
//OUT: 2
//OUT: 0
//OUT: -1

// Modulo built from division so the success path is a real computation; a
// zero divisor takes the `error.Name` return path instead.
fn checked_mod(a: i64, b: i64) !i64 {
    if (b == 0) {
        return error.DivByZero;
    }
    return a - (a / b) * b;
}

pub fn main() void {
    print(checked_mod(17, 5) catch 0 - 1);   // 17 mod 5 = 2
    print(checked_mod(9, 3) catch 0 - 1);    // a zero payload is success: 0
    print(checked_mod(4, 0) catch 0 - 1);    // error path -> -1
}
