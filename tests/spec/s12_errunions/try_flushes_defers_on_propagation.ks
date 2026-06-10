//SPEC: §12.3 `try` propagation flushes active defers before the error return — errdefers included, LIFO-merged (§21.2); success flushes only defers
//OUT: 300
//OUT: 100
//OUT: 22
//OUT: 300
//OUT: 200
//OUT: 100
//OUT: -1

fn step(n: i64) !i64 {
    if (n > 2) {
        return error.TooBig;
    }
    return n + 10;
}

// Registration order: defer 100, errdefer 200, defer 300.
// Success exit  -> reverse order, defers only:        300, 100
// try-error edge -> reverse order, errdefer included: 300, 200, 100
fn chain(n: i64) !i64 {
    defer print(100);
    errdefer print(200);
    defer print(300);
    var v: i64 = try step(n);
    return v * 2;
}

pub fn main() void {
    print(chain(1) catch 0 - 1);   // 300, 100 flush, then 22
    print(chain(5) catch 0 - 1);   // 300, 200, 100 flush, then -1
}
