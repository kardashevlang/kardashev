//SPEC: §15.1 `*T` is an ordinary value type — usable as both parameter and return type of a function
//OUT: 3
//OUT: 42

// `max_ptr` picks the pointer to the larger pointee; the increment through
// the returned pointer must land in `b` (9 > 3) and leave `a` alone.
fn max_ptr(p: *i64, q: *i64) *i64 {
    if (p.* > q.*) {
        return p;
    }
    return q;
}

pub fn main() void {
    var a: i64 = 3;
    var b: i64 = 9;
    var big: *i64 = max_ptr(&a, &b);
    big.* = big.* + 33;
    print(a);
    print(b);
}
