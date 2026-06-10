//SPEC: §23.1 a string literal is a `[]u8` — `.len`, byte indexing, and `[lo..hi]` are the §15.2 slice ops
//OUT: 9
//OUT: 953
//OUT: ashe

pub fn main() void {
    const s: []u8 = "kardashev";
    print(s.len);

    // Byte checksum derived by indexing every position:
    // k107 a97 r114 d100 a97 s115 h104 e101 v118 = 953.
    var sum: i64 = 0;
    var i: usize = 0;
    while (i < s.len) : (i += 1) {
        sum = sum + @as(i64, s[i]);
    }
    print(sum);

    var mid: []u8 = s[4..8]; // bytes 4..7
    print(mid);
}
