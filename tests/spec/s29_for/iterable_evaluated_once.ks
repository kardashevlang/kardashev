//SPEC: §29.2 the iterable expression is evaluated exactly once — a side-effecting call iterable fires once, not per element
//OUT: 7
//OUT: 12

fn make() [3]i64 {
    print(7);                      // must appear exactly once
    return [3]i64{ 2, 4, 6 };
}

pub fn main() void {
    var sum: i64 = 0;
    for (make()) |v| {
        sum += v;
    }
    print(sum);                    // 2 + 4 + 6 = 12
}
