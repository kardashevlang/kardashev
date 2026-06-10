//SPEC: §14.1 `N` is any non-negative literal — a zero-length array has an empty literal and `.len == 0`
//OUT: 0
//OUT: 0

pub fn main() void {
    var a: [0]i64 = [0]i64{};
    print(a.len);
    // A loop bounded by `.len` never runs its body.
    var visits: i64 = 0;
    var i: usize = 0;
    while (i < a.len) : (i = i + 1) {
        visits = visits + 1;
    }
    print(visits);
}
