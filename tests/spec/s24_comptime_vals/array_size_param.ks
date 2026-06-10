//SPEC: §24.2 an `[n]T` parameter resolves `n` through the bound comptime value — each size is a distinct array type
//OUT: 30
//OUT: 1111

// `xs: [n]i64` becomes `[2]i64` / `[4]i64` per instantiation; the call sites
// pass exact-size array literals (a wrong size would be an argument-type
// mismatch) and the sum walks every element, so the resolved length is
// observable in the totals.
fn sum(comptime n: i64, xs: [n]i64) i64 {
    var s: i64 = 0;
    var i: usize = 0;
    while (i < xs.len) : (i += 1) {
        s = s + xs[i];
    }
    return s;
}

pub fn main() void {
    print(sum(2, [2]i64{ 10, 20 })); // 30
    print(sum(4, [4]i64{ 1, 10, 100, 1000 })); // 1111
}
