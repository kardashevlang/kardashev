//SPEC: §27 each of `+= -= *= /= %=` on a simple-name local means `place = place op rhs`
//OUT: 12
//OUT: 9
//OUT: 36
//OUT: 7
//OUT: 3

// One running value passes through all five operators; every intermediate is
// printed so a wrong op-mapping (e.g. `-=` lowered as Add) shifts the chain.
pub fn main() void {
    var x: i64 = 7;
    x += 5;        // 7 + 5
    print(x);      // 12
    x -= 3;        // 12 - 3
    print(x);      // 9
    x *= 4;        // 9 * 4
    print(x);      // 36
    x /= 5;        // 36 / 5, integer division
    print(x);      // 7
    x %= 4;        // 7 % 4
    print(x);      // 3
}
