//SPEC: §15.2 `a[lo..hi]` on an array builds a `[]T` view with len hi-lo whose elements are the array's
//OUT: 4
//OUT: 20
//OUT: 9

pub fn main() void {
    var data: [6]i64 = [6]i64{ 4, 1, 9, 2, 8, 5 };
    var s: []i64 = data[1..5]; // views 1, 9, 2, 8
    print(s.len);

    var sum: i64 = 0;
    var i: usize = 0;
    while (i < s.len) : (i += 1) {
        sum = sum + s[i];
    }
    print(sum);          // 1 + 9 + 2 + 8
    print(s[0] + s[3]);  // first/last of the window: 1 + 8
}
