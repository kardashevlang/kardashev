//SPEC: §23.1 `s[i]` reads one byte typed `u8` — it binds to a `u8` without a cast and supports `u8` arithmetic
//OUT: 107
//OUT: 75
//OUT: 122

pub fn main() void {
    var s: []u8 = "kz";
    var c: u8 = s[0];         // 'k' = 107; the element IS u8, no @as needed
    print(c);
    var upper: u8 = c - 32;   // 'K' = 75 — u8 arithmetic on the byte
    print(upper);
    print(s[s.len - 1]);      // computed index: 'z' = 122
}
