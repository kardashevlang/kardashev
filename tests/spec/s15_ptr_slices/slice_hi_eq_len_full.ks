//SPEC: §15.2 hi == len is in bounds — `a[0..N]` views the whole array
//OUT: 5
//OUT: 30

pub fn main() void {
    var data: [5]i64 = [5]i64{ 2, 4, 6, 8, 10 };
    var s: []i64 = data[0..5]; // the full view: hi equals the array length
    print(s.len);

    var sum: i64 = 0;
    var i: usize = 0;
    while (i < s.len) : (i += 1) {
        sum = sum + s[i];
    }
    print(sum); // 2 + 4 + 6 + 8 + 10
}
