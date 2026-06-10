//SPEC: §24.2 a comptime value argument must be a compile-time constant — a runtime variable is E0253
//ERR: E0253

fn scale(comptime n: i64, x: i64) i64 {
    return n * x;
}

pub fn main() void {
    var k: i64 = 3;
    print(scale(k, 2)); // `k` is a runtime local, not a constant
}
