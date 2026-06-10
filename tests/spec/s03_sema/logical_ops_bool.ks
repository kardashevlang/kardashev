//SPEC: §3 `and`/`or` take `bool` operands and yield `bool`; `!` negates a `bool`
//OUT: 2
//OUT: 400
fn is_leap(y: i64) bool {
    return (y % 4 == 0 and y % 100 != 0) or y % 400 == 0;
}
pub fn main() void {
    // Leap years in 1896..=1904: 1896 and 1904 (1900 is divisible by 100
    // but not 400). If `and`/`or` mis-combined, the count would change.
    var count: i64 = 0;
    var y: i64 = 1896;
    while (y <= 1904) : (y = y + 1) {
        if (is_leap(y)) {
            count = count + 1;
        }
    }
    print(count);
    if (!is_leap(1900) and is_leap(2000)) {
        print(400);
    }
}
