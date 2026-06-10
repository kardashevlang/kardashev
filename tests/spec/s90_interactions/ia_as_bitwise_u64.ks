//SPEC: §33 x §28 x u64 — shifts/masks on u64 (logical, full width) interleaved with @as truncation and widening
//OUT: 8
//OUT: 51966
//OUT: 254
//OUT: 4611686018427387904
//OUT: 24656358095469135

// u64 literals above i64::MAX are not writable, so every big value is BUILT
// by shifting — which is itself under test. uint64_t shifts are logical.
pub fn main() void {
    var one: u64 = 1;
    var hi: u64 = one << 63;             // 2^63 (top bit set)
    print(hi >> 60);                     // logical: 2^63 >> 60 = 8

    var mask: u64 = (one << 16) - 1;     // 0xFFFF
    var x: u64 = 51966;                  // 0xCAFE
    print(x & mask);                     // identity under the low mask
    print(@as(u8, x));                   // truncation: 0xCAFE -> 0xFE = 254

    print(@as(i64, hi >> 1));            // 2^62 widens into i64 exactly

    // FNV-style mix step, all in u64: (seed ^ byte) * prime.
    var h: u64 = 1469598103;
    h = (h ^ 66) * 16777619;
    print(h);                            // 1469598165 * 16777619 = 24656358095469135
}
