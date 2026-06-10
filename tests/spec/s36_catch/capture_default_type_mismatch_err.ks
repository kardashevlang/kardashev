//SPEC: §36.1 the capturing handler must yield the payload type `T` — a mismatched default is E0110
//ERR: E0110

fn bad() !i64 {
    return error.Oops;
}

pub fn main() void {
    var v: i64 = bad() catch |e| true;   // bool is not the i64 payload
    print(v);
}
