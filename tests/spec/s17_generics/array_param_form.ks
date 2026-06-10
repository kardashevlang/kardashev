//SPEC: §17.1 the composite form `[N]T` (literal length) substitutes through a generic param
//OUT: 24
//OUT: 222
//OUT: 32

// A literal-sized array parameter whose element type is the type parameter.
// Each instantiation must produce a [3]<concrete> parameter — the loop walks
// all three elements by value.
fn sum3(comptime T: type, a: [3]T) T {
    var s: T = 0;
    var i: usize = 0;
    while (i < 3) : (i = i + 1) {
        s = s + a[i];
    }
    return s;
}

fn weigh3(comptime T: type, a: [3]T, w: T) T {
    // first + w*second + w*w*third — order-sensitive, so element access
    // through the substituted array type is really exercised.
    return a[0] + w * a[1] + w * w * a[2];
}

pub fn main() void {
    print(sum3(i32, [3]i32{ 7, 8, 9 }));            // 24
    print(sum3(i64, [3]i64{ 100, 101, 21 }));       // 222
    print(weigh3(i64, [3]i64{ 2, 3, 6 }, 2));       // 2 + 6 + 24 = 32
}
