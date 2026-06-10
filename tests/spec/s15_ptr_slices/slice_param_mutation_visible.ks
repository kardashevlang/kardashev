//SPEC: §15.2 passing a slice to a function passes the view — callee writes are visible in the caller's array
//OUT: 48

// If the slice were passed as a deep copy, the array would still sum to 24.
fn double_all(s: []i64) void {
    var i: usize = 0;
    while (i < s.len) : (i += 1) {
        s[i] = s[i] * 2;
    }
}

pub fn main() void {
    var data: [4]i64 = [4]i64{ 3, 5, 7, 9 };
    double_all(data[0..4]);
    print(data[0] + data[1] + data[2] + data[3]); // 2 * (3+5+7+9)
}
