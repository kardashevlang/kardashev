//SPEC: §16 `free(a, s)` releases an alloc'd slice — repeated alloc/use/free cycles stay correct
//OUT: 35

pub fn main() void {
    var a: Allocator = c_allocator();
    var total: i64 = 0;

    // Five rounds, each with its own short-lived allocation: round r holds
    // 1..r summing to r(r+1)/2, so total = 1 + 3 + 6 + 10 + 15 = 35.
    var round: usize = 1;
    while (round <= 5) : (round += 1) {
        var s: []i64 = alloc(a, i64, round);
        var i: usize = 0;
        while (i < s.len) : (i += 1) {
            s[i] = @as(i64, i) + 1;
        }
        i = 0;
        while (i < s.len) : (i += 1) {
            total = total + s[i];
        }
        free(a, s);
    }
    print(total);
}
