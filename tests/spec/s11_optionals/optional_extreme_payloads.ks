//SPEC: §11 `?T` round-trips boundary payloads faithfully (i64 extremes, u8 0 and 255)
//OUT: 9223372036854775807
//OUT: -9223372036854775808
//OUT: 255
//OUT: 0

fn wrap(n: i64) ?i64 {
    return n;
}

pub fn main() void {
    var maxv: i64 = 9223372036854775807;
    var minv: i64 = (0 - maxv) - 1;     // i64::MIN, computed without overflow
    print(wrap(maxv).?);
    print(wrap(minv).?);
    var b: ?u8 = 255;                   // u8 upper bound
    print(b orelse 0);
    b = 0;                              // u8 lower bound: present, not null
    print(b.?);
}
