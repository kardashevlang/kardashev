//SPEC: §16 `alloc(a, T, 0)` succeeds with a length-0 slice — n == 0 never trips the OOM panic
//OUT: 0
//OUT: 9
//OUT: 123

pub fn main() void {
    var a: Allocator = c_allocator();
    var s: []i64 = alloc(a, i64, 0);
    print(s.len);

    // Iterating the empty allocation runs zero times.
    var sum: i64 = 9;
    var i: usize = 0;
    while (i < s.len) : (i += 1) {
        sum = sum + s[i];
    }
    print(sum);

    free(a, s);
    print(123); // reached: no panic anywhere above
}
