//SPEC: §38 `@as(int, f64)` truncates toward zero for both signs
//OUT: 2
//OUT: -2
//OUT: 7

pub fn main() void {
    var p: f64 = 2.9;
    print(@as(i64, p));
    var n: f64 = 0.0 - 2.9;
    print(@as(i64, n));
    var w: f64 = 7.0;
    print(@as(i32, w));
}
