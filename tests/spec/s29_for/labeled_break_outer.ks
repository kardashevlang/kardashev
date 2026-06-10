//SPEC: §29.1+§40 a `for` loop may carry a label — `break :outer` from the inner loop leaves BOTH loops
//OUT: 10
//OUT: 20
//OUT: 30
//OUT: 20
//OUT: 220

pub fn main() void {
    var found: i64 = 0;
    outer: for ([2]i64{ 1, 2 }) |a| {
        for ([3]i64{ 10, 20, 30 }) |b| {
            if (a * b == 40) {
                found = a * 100 + b;
                break :outer;       // skips a=2/b=30 AND the rest of a=2
            }
            print(a * b);           // 10, 20, 30 (a=1); 20 (a=2)
        }
    }
    print(found);                   // 220
}
