//SPEC: §13.2 exactly one `switch` arm runs per execution — no C-style fallthrough into later arms
//OUT: 1111

// Each scrutinee value contributes exactly one power of ten. With
// fallthrough, n = 0 would also add 10, 100 and 1000 (total 3343);
// the correct sum 1 + 10 + 100 + 1000 = 1111 pins one-arm-only.
pub fn main() void {
    var acc: i64 = 0;
    var n: i64 = 0;
    while (n < 4) : (n = n + 1) {
        switch (n) {
            0 => { acc = acc + 1; },
            1 => { acc = acc + 10; },
            2 => { acc = acc + 100; },
            else => { acc = acc + 1000; },
        }
    }
    print(acc);
}
