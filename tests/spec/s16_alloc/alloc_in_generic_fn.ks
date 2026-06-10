//SPEC: §16 `alloc(a, T, n)` inside a generic fn resolves T through the active substitution — one alloc per instantiation
//OUT: 9
//OUT: 12
//OUT: 18

// `make_seq` is monomorphised at i32 and i64; the alloc's type argument is
// the *type parameter*, so each copy must allocate its own element type.
fn make_seq(comptime T: type, a: Allocator, n: usize) []T {
    var s: []T = alloc(a, T, n);
    var i: usize = 0;
    while (i < n) : (i += 1) {
        s[i] = @as(T, i * 3);
    }
    return s;
}

pub fn main() void {
    var a: Allocator = c_allocator();

    var xs: []i32 = make_seq(i32, a, 4); // 0 3 6 9
    var ys: []i64 = make_seq(i64, a, 5); // 0 3 6 9 12
    print(xs[3]);
    print(ys[4]);

    var sum: i32 = 0;
    var i: usize = 0;
    while (i < xs.len) : (i += 1) {
        sum = sum + xs[i];
    }
    print(sum); // 0+3+6+9

    free(a, xs);
    free(a, ys);
}
