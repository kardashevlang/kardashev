//SPEC: §28.2+§28.3 u64 is a full 64-bit lane — `~0` is all-ones and `>>` on it is a logical (zero-filling) shift
//OUT: 15
//OUT: 9223372036854775807
//OUT: 2
//OUT: 0
//OUT: 255
//OUT: 0

pub fn main() void {
    var ones: u64 = 0;
    ones = ~ones;              // 2^64 - 1 — needs all 64 bits
    print(ones >> 60);         // 15: the top nibble is set, and >> shifts in zeros
    print(ones >> 1);          // 2^63 - 1 = 9223372036854775807
    var top: u64 = ones - (ones >> 1);  // 2^63: only the top bit set
    print(top >> 62);          // 2 — logical shift of a top-bit-set u64
    print((top >> 63) - 1);    // 1 - 1 = 0 — the bit landed exactly at position 0
    print(ones & 255);         // low byte all-ones
    print(ones ^ ones);        // 0
}
