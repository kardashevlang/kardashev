//SPEC: §38.x f64 arithmetic is deterministic cross-platform: a*b+c is never FMA-fused (-ffp-contract=off), so 0.1*1e17+0.5 double-rounds identically everywhere
//OUT: 0.10000000000000000
@import("std");
pub fn main() void {
    const a = c_allocator();
    // The exact case that diverged on Apple clang's default contraction:
    // round(0.1 * 1e17) = 1e16 exactly, then +0.5 truncates back — fused,
    // the single rounding lands on 1e16 + 2 and the last digit becomes 2.
    var s: []u8 = fmt_f64(a, 0.1, 17);
    print(s);
    free(a, s);
}
