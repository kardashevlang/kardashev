//SPEC: §14.1 arrays pass into and return from functions by value — the result is an independent copy
//OUT: 8
//OUT: 16

// `same` returns its parameter unchanged. If parameter passing or returning
// aliased the caller's storage, mutating `b` would corrupt `a`.
fn same(a: [3]i64) [3]i64 {
    return a;
}

pub fn main() void {
    var a: [3]i64 = [3]i64{ 7, 8, 9 };
    var b: [3]i64 = same(a);
    b[1] = 0;
    print(a[1]);                  // still 8: the round-trip made a copy
    print(b[0] + b[1] + b[2]);    // 7 + 0 + 9
}
