//SPEC: §11.1 the `orelse` alternative is a full `T` expression, so chains nest as `a orelse (b orelse k)`
//OUT: 15
//OUT: 2
//OUT: 7

// `orelse` requires a plain-`T` right operand, so a two-level fallback chain
// is written with the inner `orelse` parenthesized (its result is the T the
// outer alternative needs). All three selection outcomes are exercised.
fn lookup(n: i64) ?i64 {
    if (n >= 10) {
        return n - 10;
    }
    return null;
}

pub fn main() void {
    print(lookup(25) orelse (lookup(12) orelse 7));   // first hit: 15
    print(lookup(3) orelse (lookup(12) orelse 7));    // second hit: 2
    print(lookup(3) orelse (lookup(4) orelse 7));     // final default: 7
}
