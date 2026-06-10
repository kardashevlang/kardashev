//SPEC: §29 `for` iterates a slice's elements in order; the index form and a slice expression as the iterable both work
//OUT: 9
//OUT: 12
//OUT: 6

pub fn main() void {
    var xs: [5]i64 = [5]i64{ 3, 1, 4, 1, 5 };
    var s: []i64 = xs[0..4];      // view of {3, 1, 4, 1}
    var sum: i64 = 0;
    for (s) |v| {
        sum += v;
    }
    print(sum);                   // 3 + 1 + 4 + 1 = 9
    // Index form over the slice: 3*0 + 1*1 + 4*2 + 1*3 = 12.
    var weighted: i64 = 0;
    for (s, 0..) |v, i| {
        weighted += v * @as(i64, i);
    }
    print(weighted);
    // The iterable may be a slice expression directly.
    var tail: i64 = 0;
    for (xs[3..5]) |v| {
        tail += v;                // 1 + 5
    }
    print(tail);                  // 6
}
