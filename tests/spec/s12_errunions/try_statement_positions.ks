//SPEC: §12.1 `try` is allowed as the whole value of a var/const initializer, a `return`, or an expression statement
//OUT: 3
//OUT: 6
//OUT: 7
//OUT: 14
//OUT: -1

fn gate(n: i64) !i64 {
    if (n < 0) {
        return error.Negative;
    }
    print(n);            // a witness per successful gate
    return n * 2;
}

fn use_all(n: i64) !i64 {
    var a: i64 = try gate(n);     // initializer position
    try gate(a);                  // expression-statement position (payload discarded)
    return try gate(a + 1);       // return position
}

pub fn main() void {
    print(use_all(3) catch 0 - 1);       // gates print 3, 6, 7; result 14
    print(use_all(0 - 9) catch 0 - 1);   // first gate errors: no prints, -1
}
