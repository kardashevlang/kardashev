//SPEC: §15.1 a deref `p.*` is itself an lvalue — `&p.*` re-takes the pointee's address
//OUT: 42

// `q` is built from `&p.*`, so it must alias the same variable `x`; the write
// through `q` is observed reading `x` directly.
pub fn main() void {
    var x: i64 = 6;
    var p: *i64 = &x;
    var q: *i64 = &p.*;
    q.* = q.* * 7; // 6 * 7
    print(x);
}
