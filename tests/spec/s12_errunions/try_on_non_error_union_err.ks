//SPEC: §12.1 the `try` operand must be an error union (`!T`); a plain value is E0190
//ERR: E0190

fn g() !i64 {               // the enclosing fn DOES return !T — only the
    var n: i64 = 4;         // operand is wrong here
    var x: i64 = try n;
    return x;
}

pub fn main() void {
    print(g() catch 0);
}
