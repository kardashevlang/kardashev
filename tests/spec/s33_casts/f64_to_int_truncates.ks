//SPEC: §33/§38 `@as(<int>, x)` on an `f64` truncates toward zero (the documented C-cast lowering), for negative values too and into any integer width
//OUT: 3
//OUT: -3
//OUT: 2
//OUT: 7
//OUT: 100
pub fn main() void {
    var h: f64 = @as(f64, 7) / @as(f64, 2);   // 3.5
    print(@as(i64, h));                       // 3
    var n: f64 = 0.0 - h;                     // -3.5
    print(@as(i64, n));                       // -3, toward zero (not -4)
    print(@as(i64, 2.999));                   // 2 — truncation, not rounding
    var u: u8 = @as(u8, 7.9);                 // float into a narrow unsigned
    print(@as(i64, u));                       // 7
    print(@as(i32, 100.0));                   // 100 — exact whole value
}
