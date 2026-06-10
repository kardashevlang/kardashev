//SPEC: §12.1 `try` is statement-level only: nested inside a larger expression it is E0191
//ERR: E0191

fn f() !i64 {
    return 1;
}

fn g() !i64 {
    var x: i64 = (try f()) + 1;   // not the WHOLE initializer value
    return x;
}

pub fn main() void {
    print(g() catch 0);
}
