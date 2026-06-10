//SPEC: §29 `for` loops nest — the inner loop runs to completion for every outer element
//OUT: 115
//OUT: 6

pub fn main() void {
    var as_: [2]i64 = [2]i64{ 2, 3 };
    var bs: [3]i64 = [3]i64{ 5, 7, 11 };
    var sum: i64 = 0;
    var pairs: i64 = 0;
    for (as_) |a| {
        for (bs) |b| {
            sum += a * b;
            pairs += 1;
        }
    }
    print(sum);     // (2+3) * (5+7+11) = 115
    print(pairs);   // 2 * 3 = 6
}
