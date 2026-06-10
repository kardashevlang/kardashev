//SPEC: §36.2 the capturing handler runs ONLY on the error path — even a diverging `@panic` handler is skipped on success
//OUT: 42

fn ok() !i64 {
    return 42;
}

pub fn main() void {
    // Stronger than a side-effect witness: were the handler evaluated on the
    // ok path (as the non-capturing form's eager default would be, §12.1),
    // the program would exit 101 and print nothing.
    print(ok() catch |e| @panic("handler must not run"));
}
