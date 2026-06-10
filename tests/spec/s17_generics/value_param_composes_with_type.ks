//SPEC: §24.1 comptime type and value parameters compose — `[n]T` sizes per instantiation
//OUT: 110
//OUT: 50
//OUT: 60

// One generic, monomorphised per (T, n) pair: (i32, 2), (i64, 3), (i64, 4).
// The value parameter both sizes the array parameters and bounds the loop, so
// a wrong binding of `n` changes the sum.
fn dot(comptime T: type, comptime n: usize, a: [n]T, b: [n]T) T {
    var s: T = 0;
    var i: usize = 0;
    while (i < n) : (i = i + 1) {
        s = s + a[i] * b[i];
    }
    return s;
}

pub fn main() void {
    print(dot(i32, 2, [2]i32{ 3, 4 }, [2]i32{ 10, 20 }));          // 30 + 80 = 110
    print(dot(i64, 3, [3]i64{ 1, 2, 3 }, [3]i64{ 7, 8, 9 }));      // 7 + 16 + 27 = 50
    print(dot(i64, 4, [4]i64{ 1, 1, 1, 1 }, [4]i64{ 9, 13, 17, 21 }));  // 60
}
