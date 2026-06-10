//SPEC: §24.2 type and value comptime parameters mix in one signature; the instantiation key is the ordered comptime arguments
//OUT: 30
//OUT: 5

// `nth` is keyed on (T, n): the two calls instantiate (i64, 3) and (i32, 2).
// `n` sizes the array parameter AND indexes inside the body (the constant
// participates in the index arithmetic), so each key's behaviour is distinct.
fn nth(comptime T: type, comptime n: usize, xs: [n]T) T {
    return xs[n - 1];
}

pub fn main() void {
    print(nth(i64, 3, [3]i64{ 10, 20, 30 })); // last of 3 = 30
    print(nth(i32, 2, [2]i32{ 4, 5 })); // last of 2 = 5
}
