//SPEC: §12 one implicit global error set: an error from a `!u8` function propagates through a `!i64` function unchanged
//OUT: 1007
//OUT: -1
//OUT: -1
//OUT: 0

fn small(n: i64) !u8 {
    if (n > 255) {
        return error.TooBig;
    }
    if (n < 0) {
        return error.Negative;
    }
    return @as(u8, n);
}

// `try` carries the error across DIFFERENT payload types (!u8 -> !i64):
// error values live in one program-wide set, independent of the payload.
fn boost(n: i64) !i64 {
    var b: u8 = try small(n);
    return @as(i64, b) + 1000;
}

pub fn main() void {
    print(boost(7) catch 0 - 1);       // 7 + 1000 = 1007
    print(boost(300) catch 0 - 1);     // TooBig propagated -> -1
    print(boost(0 - 4) catch 0 - 1);   // Negative propagated -> -1
    // Distinct names get distinct codes even across the payload boundary.
    var c1: i64 = boost(300) catch |e| @as(i64, e);
    var c2: i64 = boost(0 - 4) catch |e| @as(i64, e);
    if (c1 == c2) {
        print(1);
    } else {
        print(0);
    }
}
