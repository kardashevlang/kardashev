//SPEC: §29.1 `for` captures |x, i| shadow same-named outer vars inside the body only — the outers are untouched after the loop
//OUT: 243
//OUT: 100
//OUT: 555

pub fn main() void {
    var x: i64 = 100;     // shadowed by the element capture
    var i: i64 = 555;     // shadowed by the index capture

    var xs: [3]i64 = [3]i64{ 7, 8, 9 };
    var total: i64 = 0;
    for (xs, 0..) |x, i| {
        // x is the element (7,8,9), i the usize index (0,1,2):
        // 70+0 + 80+1 + 90+2 = 243. Were the outers read instead, the
        // total would be 3 * (1000 + 555) = 4665.
        total += x * 10 + @as(i64, i);
    }
    print(total);     // 243
    print(x);         // outer x intact
    print(i);         // outer i intact
}
