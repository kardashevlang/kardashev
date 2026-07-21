//SPEC: §40.2 `continue :outer` targeting a labeled for still advances the iteration (the induction increments)
//OUT: 0
//OUT: 4
//OUT: 3

pub fn main() void {
    var xs: [3]i64 = [3]i64{ 0, 5, 2 };
    var hits: i64 = 0;
    outer: for (xs, 0..) |x, i| {
        if (x == 5) { continue :outer; }
        print(@as(i64, i) + x);   // 0+0, then 2+2
        hits += 1;
    }
    print(hits + 1);   // two hits + 1
}
