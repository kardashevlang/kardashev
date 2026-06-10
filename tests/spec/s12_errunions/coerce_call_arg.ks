//SPEC: §12.2 `T -> !T` and `error.X -> !T` coerce at a call argument whose param is `!T`
//OUT: 18
//OUT: -1
//OUT: 0

fn settle(r: !i64) i64 {
    return r catch |e| 0 - @as(i64, e);
}

pub fn main() void {
    var n: i64 = 4;
    print(settle(n * n + 2));    // a plain i64 expression coerces: 18
    print(settle(error.Only));   // the sole error name (code 1) coerces: -1
    print(settle(n - 4));        // a ZERO payload is success (0 = "no error"
                                 // is the err field, not the payload): 0
}
