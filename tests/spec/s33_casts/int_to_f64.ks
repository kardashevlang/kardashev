//SPEC: §33/§38 `@as(f64, n)` converts an integer to `f64` — the only way to mix, since there is no implicit int↔float coercion
//OUT: 3
//OUT: 3.5
//OUT: -5
//OUT: 0.25
pub fn main() void {
    print(@as(f64, 3));                       // exact: prints without a fraction
    var h: f64 = @as(f64, 7) / @as(f64, 2);   // both operands made f64 explicitly
    print(h);                                 // 3.5
    var n: i64 = 0 - 5;
    print(@as(f64, n));                       // -5
    var q: f64 = @as(f64, 1) / @as(f64, 4);
    print(q);                                 // 0.25 — real float division, not 0
}
