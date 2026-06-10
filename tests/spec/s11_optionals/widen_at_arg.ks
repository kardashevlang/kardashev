//SPEC: §11.2 a `T` value widens to `?T` at a call argument whose param is `?T`; an existing `?T` passes through
//OUT: 42
//OUT: 200
//OUT: 0
//OUT: 14

fn score(v: ?i64) i64 {
    return (v orelse 0) * 2;
}

pub fn main() void {
    var n: i64 = 10;
    print(score(n + 11));      // plain i64 expression widens: (21) * 2 = 42
    print(score(n * n));       // (100) * 2 = 200
    var already: ?i64 = null;
    print(score(already));     // already ?i64: passes through unchanged -> 0
    already = 7;
    print(score(already));     // 7 * 2 = 14
}
