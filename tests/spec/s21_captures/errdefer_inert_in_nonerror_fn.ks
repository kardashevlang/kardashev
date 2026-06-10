//SPEC: §21.2 `errdefer` is accepted in any function — it never fires in one that cannot return an error
//OUT: 11
//OUT: 10

// `tally` returns a plain i64: there is no error-return edge at all, so the
// errdefer is legal but inert. Were it treated as a plain defer, 99 would
// print between 11 and 10.
fn tally(n: i64) i64 {
    errdefer print(99);
    defer print(11);
    var s: i64 = 0;
    var i: i64 = 1;
    while (i <= n) : (i = i + 1) {
        s = s + i;
    }
    return s;                   // 1+2+3+4 = 10, after the defer prints 11
}

pub fn main() void {
    print(tally(4));
}
