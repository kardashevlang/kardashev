//SPEC: §14.2 a negative array length is rejected
//ERR: E0224

// The only source-level route to a non-literal length: a comptime value
// parameter (§24) bound to a negative constant at the call site.
fn head(comptime n: i64, xs: [n]i64) i64 {
    return xs[0];
}

pub fn main() void {
    print(head(-1, [1]i64{ 5 }));
}
