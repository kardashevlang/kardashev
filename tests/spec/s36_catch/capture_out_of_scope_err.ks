//SPEC: §36.1 the capture is bound ONLY inside the handler — `e` after the catch expression is an unknown name (E0100)
//ERR: E0100

fn bad() !i32 {
    return error.Oops;
}

pub fn main() void {
    var v: i32 = bad() catch |e| 0;
    print(e);   // out of scope
}
