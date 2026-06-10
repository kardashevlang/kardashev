//SPEC: §14.1 `a.len` is the array's length, a `usize` constant
//OUT: 5
//OUT: 30

pub fn main() void {
    var a: [5]i64 = [5]i64{ 2, 4, 6, 8, 10 };
    var n: usize = a.len;     // binds as a usize value
    print(n);
    // `.len` bounds the loop (usize counter compared against it).
    var sum: i64 = 0;
    var i: usize = 0;
    while (i < a.len) : (i = i + 1) {
        sum = sum + a[i];
    }
    print(sum);
}
