//SPEC: §36.2 + §21.2 the callee's defers AND errdefers flush (reverse order) during the error return, BEFORE the caller's catch handler runs
//OUT: 60
//OUT: 50
//OUT: 70
//OUT: 1

fn bad(n: i64) !i64 {
    errdefer print(50);   // registered first
    defer print(60);      // registered second -> flushes first
    if (n > 2) {
        return error.Big;
    }
    return n;
}

fn handle(e: i32) i64 {
    print(70);            // the handler's witness — must come AFTER 60, 50
    // One error name in the program -> its 1-based code (§12.1) is 1.
    return @as(i64, e);
}

pub fn main() void {
    var v: i64 = bad(9) catch |e| handle(e);
    print(v);
}
