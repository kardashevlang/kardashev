//SPEC: §15.1 `p.* = e` (deref place assignment) writes through the pointer into the pointee
//OUT: 48

// Four doublings applied only through the pointer; the variable itself is
// never assigned directly. 3 * 2^4 = 48.
pub fn main() void {
    var x: i64 = 3;
    var p: *i64 = &x;
    var i: i64 = 0;
    while (i < 4) : (i += 1) {
        p.* = p.* * 2;
    }
    print(x);
}
