//SPEC: §17.1 the composite form `?T` substitutes through a generic return type
//OUT: 4
//OUT: -1
//OUT: 1000000

// A generic returning `?T`: the found element comes back unwrappable with
// `orelse`, the not-found path is `null`. Two instantiations so each gets its
// own `?T` specialisation.
fn find(comptime T: type, xs: []T, want: T) ?T {
    var i: usize = 0;
    while (i < xs.len) : (i = i + 1) {
        if (xs[i] == want) {
            return xs[i];
        }
    }
    return null;
}

pub fn main() void {
    var a: [4]i32 = [4]i32{ 3, 1, 4, 1 };
    print(find(i32, a[0..4], 4) orelse 0 - 1);      // hit -> 4
    print(find(i32, a[0..4], 9) orelse 0 - 1);      // miss -> null -> -1

    var b: [3]i64 = [3]i64{ 10, 1000000, 7 };
    print(find(i64, b[0..3], 1000000) orelse 0);    // hit at i64 -> 1000000
}
