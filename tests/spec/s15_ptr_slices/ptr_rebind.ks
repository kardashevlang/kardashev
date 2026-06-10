//SPEC: §15.1 a `*T` variable is a first-class value — reassigning it retargets later derefs
//OUT: 4
//OUT: 103

// The pointer increments `a` for three iterations, is rebound, then
// increments `b` for the remaining three. If rebinding silently kept the old
// target, `a` would end at 7 and `b` at 100.
pub fn main() void {
    var a: i64 = 1;
    var b: i64 = 100;
    var p: *i64 = &a;
    var i: i64 = 0;
    while (i < 6) : (i += 1) {
        p.* = p.* + 1;
        if (i == 2) {
            p = &b;
        }
    }
    print(a); // 1 + 3
    print(b); // 100 + 3
}
