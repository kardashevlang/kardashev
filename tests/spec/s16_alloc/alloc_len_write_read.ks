//SPEC: §16 `alloc(a, T, n)` returns a `[]T` of length exactly n — every element writable and readable back
//OUT: 6
//OUT: 16
//OUT: 55

pub fn main() void {
    var a: Allocator = c_allocator();
    var n: usize = 6;
    var s: []i64 = alloc(a, i64, n);
    print(s.len);

    var i: usize = 0;
    while (i < s.len) : (i += 1) {
        s[i] = @as(i64, i) * @as(i64, i); // 0 1 4 9 16 25
    }
    print(s[4]);

    var sum: i64 = 0;
    i = 0;
    while (i < s.len) : (i += 1) {
        sum = sum + s[i];
    }
    print(sum); // 0+1+4+9+16+25

    free(a, s);
}
