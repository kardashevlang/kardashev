//SPEC: §34.2 membership is enforced at error-LITERAL sites only: a foreign error `try`-propagates through a `Set!T` function (Set!T ≡ !T at runtime)
//OUT: -7

const IoErr = error{ NotFound, Denied };

// `error.Foreign` is not a member of any set — fine in a global `!T`.
fn anyFail() !i64 {
    return error.Foreign;
}

// The propagation through a Set-typed function is NOT membership-checked:
// only literal `return error.X;` / `var x: Set!T = error.X;` sites are
// (the propagated value is not a literal). The error flows through unchanged.
fn setFn(n: i64) IoErr!i64 {
    var v: i64 = try anyFail();
    return v + n;
}

pub fn main() void {
    print(setFn(1) catch 0 - 7);   // Foreign propagated -> default -7
}
