//SPEC: §17.2 a generic body calling another generic with forwarded T instantiates it transitively (worklist)
//OUT: 50
//OUT: 221

// `square` is never called from non-generic code: its i32 and i64 instances
// exist only because checking `sum_squares`'s instances discovers the inner
// calls (instantiation is processed transitively via a worklist).
fn square(comptime T: type, x: T) T {
    return x * x;
}

fn sum_squares(comptime T: type, xs: []T) T {
    var s: T = 0;
    var i: usize = 0;
    while (i < xs.len) : (i = i + 1) {
        s = s + square(T, xs[i]);   // T forwarded to the inner generic
    }
    return s;
}

pub fn main() void {
    var a: [3]i32 = [3]i32{ 3, 4, 5 };
    print(sum_squares(i32, a[0..3]));   // 9 + 16 + 25 = 50

    var b: [2]i64 = [2]i64{ 10, 11 };
    print(sum_squares(i64, b[0..2]));   // 100 + 121 = 221
}
