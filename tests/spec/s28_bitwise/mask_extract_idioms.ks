//SPEC: §28.1 the full precedence ladder composes in mask idioms — `a | b ^ c & d`, `v >> k & m`, `flags & 1 << p`
//OUT: 15
//OUT: 1
//OUT: 8
//OUT: 5

pub fn main() void {
    // a | b ^ c & d  ≡  a | (b ^ (c & d)): 10&6=2, 12^2=14, 9|14=15.
    // Both wrong left-to-right groupings give 6.
    var a: i64 = 9;
    var b: i64 = 12;
    var c: i64 = 10;
    var d: i64 = 6;
    print(a | b ^ c & d);

    // Field extraction: v >> k & m ≡ (v >> k) & m (& is LOWER than shift).
    // 173 = 0b10101101: (173 >> 2) & 5 = 43 & 5 = 1; v >> (k & 5) would be 43.
    var v: i64 = 173;
    var k: i64 = 2;
    print(v >> k & 5);

    // Bit test: flags & 1 << p ≡ flags & (1 << p). 42 & 8 = 8;
    // (flags & 1) << p would be 0.
    var flags: i64 = 42;
    var p: i64 = 3;
    print(flags & 1 << p);

    // Popcount of 173 via the same idioms inside a loop: 5 set bits.
    var count: i64 = 0;
    var w: i64 = 173;
    while (w > 0) : (w = w >> 1) {
        count += w & 1;
    }
    print(count);
}
