//SPEC: §35.2 `@panic` in an expression position (a `catch |e|` handler) executes on the error path: exit 101 after the pre-error output
//EXIT: 101
//OUT: 7

fn bad() !i64 {
    return error.Boom;
}

pub fn main() void {
    print(7);
    // The handler is a value position (must be i64): the diverging @panic
    // adopts it, and on the error path the (kd_panic, 0) lowering runs.
    var v: i64 = bad() catch |e| @panic("giving up");
    print(v);   // never reached
}
