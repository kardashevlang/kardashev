//SPEC: §33/§38 `@as(T, e)` where `e` is already of type `T` is a value-preserving no-op for any numeric `T` (int or f64)
//OUT: -42
//OUT: 7
//OUT: 1.5
pub fn main() void {
    var n: i64 = 0 - 42;
    print(@as(i64, n));               // i64 -> i64
    var b: u8 = 7;
    print(@as(i64, @as(u8, b)));      // u8 -> u8, then widened only to print
    var x: f64 = 1.5;
    print(@as(f64, x));               // f64 -> f64
}
