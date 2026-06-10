//SPEC: §24.2 a comptime value parameter may size an array type `[n]T`, monomorphised per value
//OUT: 42
//OUT: 15

// One generic `sum` serves arrays of two different lengths; each call binds
// `n` at compile time and the parameter type [n]i64 resolves per instance.
fn sum(comptime n: usize, xs: [n]i64) i64 {
    var total: i64 = 0;
    var i: usize = 0;
    while (i < n) : (i = i + 1) {     // `n` is also usable as a value
        total = total + xs[i];
    }
    return total;
}

pub fn main() void {
    print(sum(2, [2]i64{ 40, 2 }));
    print(sum(5, [5]i64{ 1, 2, 3, 4, 5 }));
}
