//SPEC: §29 `for` over a zero-length array or an empty slice runs the body zero times
//OUT: 0
//OUT: 0
//OUT: 7

pub fn main() void {
    var empty: [0]i64 = [0]i64{};
    var visits: i64 = 0;
    for (empty) |v| {
        visits += v + 1000;       // must never run
    }
    print(visits);                // 0

    var xs: [3]i64 = [3]i64{ 1, 2, 3 };
    var none: []i64 = xs[1..1];   // lo == hi: an empty view
    for (none, 0..) |v, i| {
        visits += v + @as(i64, i) + 1000;
    }
    print(visits);                // still 0
    print(7);                     // control flow reaches past both loops
}
