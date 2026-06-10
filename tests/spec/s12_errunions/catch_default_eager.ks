//SPEC: §12.1 the non-capturing `catch` default is evaluated eagerly, even on the success path
//OUT: 111
//OUT: 7
//OUT: 111
//OUT: 50

// The default prints a witness each time it is evaluated. Per §12.1/§12.3 the
// non-capturing form lowers to a plain helper call, so 111 appears on BOTH
// paths (contrast with the capturing form, §36.2, which is lazy).
fn dflt() i64 {
    print(111);
    return 50;
}

fn ok() !i64 {
    return 7;
}

fn bad() !i64 {
    return error.Bad;
}

pub fn main() void {
    print(ok() catch dflt());    // 111 (eager), then the payload 7
    print(bad() catch dflt());   // 111, then the default 50
}
