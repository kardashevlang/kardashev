//SPEC: §21.1 `if (opt) |v|` binds the unwrapped `T` in the then-branch; the else-branch runs on null
//OUT: 18
//OUT: 2

fn odd_double(n: i64) ?i64 {
    if (n % 2 == 1) {
        return n * 2;
    }
    return null;
}

pub fn main() void {
    var sum: i64 = 0;
    var nulls: i64 = 0;
    var n: i64 = 1;
    while (n <= 5) : (n = n + 1) {
        if (odd_double(n)) |v| {
            sum = sum + v;          // v is the plain i64 payload
        } else {
            nulls = nulls + 1;      // taken exactly for n = 2, 4
        }
    }
    print(sum);     // 2 + 6 + 10 = 18
    print(nulls);   // 2
}
