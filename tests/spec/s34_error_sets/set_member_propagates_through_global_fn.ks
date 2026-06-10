//SPEC: §34.2 `Set!T` ≡ `!T` at runtime: a set member `try`-propagates through a global `!T` function unchanged
//OUT: 11
//OUT: -1

const IoErr = error{ NotFound, Denied };

fn setFail(n: i64) IoErr!i64 {
    if (n < 0) {
        return error.Denied;
    }
    return n * 2;
}

// A plain `!i64` hop: the set-typed error crosses it via `try` because the
// runtime representation is identical (the set is a compile-time constraint).
fn globalHop(n: i64) !i64 {
    var v: i64 = try setFail(n);
    return v + 1;
}

pub fn main() void {
    print(globalHop(5) catch 0 - 1);       // 5*2 + 1 = 11
    print(globalHop(0 - 3) catch 0 - 1);   // Denied propagated -> -1
}
