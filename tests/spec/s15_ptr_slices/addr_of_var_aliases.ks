//SPEC: §15.1 `&x` yields a `*T` that aliases the variable — later writes to `x` are visible through `p.*`
//OUT: 2
//OUT: 23

// If `&x` copied the value instead of taking its address, the second read
// through `p` would still be 2 rather than tracking the loop's updates.
pub fn main() void {
    var x: i64 = 2;
    var p: *i64 = &x;
    print(p.*);

    var i: i64 = 0;
    while (i < 3) : (i += 1) {
        x = x * 2 + 1; // 2 -> 5 -> 11 -> 23
    }
    print(p.*);
}
