//SPEC: §33 `@as(usize, e)` bridges signed values into `usize` (the documented motivating case: an i32 key as an index) and back out via `@as(i64, …)`
//OUT: 30
//OUT: 7
//OUT: 100000
pub fn main() void {
    var key: i32 = 2;
    var i: usize = @as(usize, key);          // the SPEC §33 example shape
    var arr: [4]i64 = [4]i64{ 10, 20, 30, 40 };
    print(arr[i]);                           // 30 — the cast value really indexes
    print(@as(i64, i) + 5);                  // 7 — usize back into i64 arithmetic
    var big: usize = @as(usize, 100000);
    print(big);                              // 100000
}
