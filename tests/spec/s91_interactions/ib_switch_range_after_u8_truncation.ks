//SPEC: §39×§33 switch range dispatch happens on the `@as(u8, n)`-truncated value — i64 inputs 256 apart land in the same arm, and ranges/multi-labels mix
//OUT: 1
//OUT: 2
//OUT: 3
//OUT: 1
//OUT: 2
//OUT: 2
//OUT: 3
//OUT: 3

fn classify(n: i64) i64 {
    var b: u8 = @as(u8, n);     // truncation: value mod 256
    switch (b) {
        0..9 => {
            return 1;
        },
        10, 20, 30 => {
            return 2;
        },
        31..255 => {
            return 3;
        },
        else => {
            return 4;
        },
    }
}

pub fn main() void {
    print(classify(5));        // 5          -> range 0..9   -> 1
    print(classify(20));       // 20         -> label list   -> 2
    print(classify(100));      // 100        -> range 31..255 -> 3
    print(classify(256));      // 256 -> 0   -> range 0..9   -> 1
    print(classify(266));      // 266 -> 10  -> label list   -> 2
    print(classify(276));      // 276 -> 20  -> label list   -> 2
    print(classify(511));      // 511 -> 255 -> range hi bound -> 3
    print(classify(0 - 1));    // -1  -> 255 (two's complement) -> 3
}
