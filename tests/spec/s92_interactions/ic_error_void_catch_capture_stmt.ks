//SPEC: §12.3 over `!void` the capturing handler runs as a statement on the error path only, binding the code
//OUT: 5
//OUT: 2
//OUT: 9

const Ops = error{ A, B };

fn work(fail: bool) Ops!void {
    if (fail) { return error.B; }
    print(5);
}

pub fn main() void {
    work(false) catch |e| print(0 - @as(i64, e));   // success: prints 5, no handler
    work(true) catch |e| print(@as(i64, e));        // error.B is code 2 (A=1, B=2)
    print(9);
}
