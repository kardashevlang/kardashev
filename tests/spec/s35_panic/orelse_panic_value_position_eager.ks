//SPEC: §35.1 `@panic` adopts the expected type in `x orelse @panic(…)`; with §11.3's EAGER orelse rhs it fires even when x is non-null
//EXIT: 101
//OUT: 1

fn find(n: i64) ?i64 {
    if (n > 0) {
        return n * 3;
    }
    return null;
}

pub fn main() void {
    print(1);
    // Type-checks: the diverging @panic adopts the i64 the orelse expects.
    // Runtime: §11.3 evaluates the orelse rhs eagerly (v0.114 semantics), so
    // the panic fires even though find(4) is non-null.
    var v: i64 = find(4) orelse @panic("absent");
    print(v);   // never reached
}
