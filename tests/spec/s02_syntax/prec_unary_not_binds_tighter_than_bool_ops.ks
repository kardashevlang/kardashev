//SPEC: §2/§28.1 unary `!` binds tighter than `and`/`or` — !f and f is (!f) and f
//OUT: 0
//OUT: 1
pub fn main() void {
    var t: bool = 0 < 1; // true, computed
    var f: bool = 1 < 0; // false, computed
    // (!f) and f = true and false = false.  Wrong grouping !(f and f) = true.
    if (!f and f) {
        print(1);
    } else {
        print(0);
    }
    // (!t) or t = false or true = true.  Wrong grouping !(t or t) = false.
    if (!t or t) {
        print(1);
    } else {
        print(0);
    }
}
