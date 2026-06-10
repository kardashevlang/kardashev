//SPEC: §36.2 the capturing `catch |e|` handler is evaluated only on the error path
//OUT: 7
//OUT: 222
//OUT: 1

// The handler prints a witness when evaluated. On the ok path 222 must NOT
// appear (the capturing form is lazy — unlike the eager non-capturing
// `catch default`, §12.1).
fn witness(e: i32) i32 {
    print(222);
    return e;
}

fn ok() !i32 {
    return 7;
}

fn bad() !i32 {
    return error.Oops;
}

pub fn main() void {
    print(ok() catch |e| witness(e));    // no 222; just the payload 7
    print(bad() catch |e| witness(e));   // 222, then the sole code 1
}
