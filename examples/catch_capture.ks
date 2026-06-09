// catch_capture.ks — the capturing error handler `catch |e|` (v0.142).
//
//   expr catch |e| default
//
// If `expr` (an `!T`) is ok, you get the payload; otherwise the error code is
// bound to `e` and `default` is evaluated only on the error path — so the
// handler can react to *which* error happened. The plain `expr catch default`
// (no `|e|`) still works unchanged.

const MathErr = error{ DivByZero, Negative };

fn safe_div(a: i32, b: i32) MathErr!i32 {
    if (b == 0) {
        return error.DivByZero;
    }
    return a / b;
}

fn isqrt_floor(n: i32) MathErr!i32 {
    if (n < 0) {
        return error.Negative;
    }
    var r: i32 = 0;
    while ((r + 1) * (r + 1) <= n) : (r += 1) {}
    return r;
}

pub fn main() i32 {
    // Success: the handler is skipped, the payload flows through.
    print(safe_div(84, 2) catch |e| (0 - e));     // 42
    print(isqrt_floor(50) catch |e| (0 - e));      // 7

    // Error: `e` is the error code (DivByZero / Negative); the handler reacts.
    print(safe_div(1, 0) catch |e| (1000 + e));    // 1000 + code(DivByZero)
    print(isqrt_floor(0 - 4) catch |e| (2000 + e)); // 2000 + code(Negative)

    // The captured code can drive a fallback value (the default is an
    // expression). Distinct error codes select distinct fallbacks here.
    var code: i32 = safe_div(10, 0) catch |e| e;
    print(code * 7);                                // code(DivByZero) * 7
    return 0;
}

test "catch capture" {
    expect((safe_div(20, 4) catch |e| (0 - e)) == 5);
    expect((safe_div(3, 0) catch |e| 0) == 0);      // handler value on error
    expect((isqrt_floor(81) catch 0) == 9);          // non-capturing still works
}
