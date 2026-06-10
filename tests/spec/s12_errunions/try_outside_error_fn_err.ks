//SPEC: §12.1 `try` requires the enclosing function to return some `!U`; otherwise E0190
//ERR: E0190

fn f() !i64 {
    return 1;
}

pub fn main() void {       // main returns void, not an error union
    var x: i64 = try f();
    print(x);
}
