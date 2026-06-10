//SPEC: §38 `f64` works as an error-union payload: `!f64` returns, `catch` fallback
//OUT: 7
//OUT: 0.125

fn checked(x: f64) !f64 {
    if (x > 100.0) {
        return error.TooBig;
    }
    return x * 2.0;
}

pub fn main() void {
    var ok: f64 = checked(3.5) catch 0.125;
    print(ok);                             // success: 7.0 prints "7"
    var bad: f64 = checked(200.0) catch 0.125;
    print(bad);                            // error path takes the fallback
}
