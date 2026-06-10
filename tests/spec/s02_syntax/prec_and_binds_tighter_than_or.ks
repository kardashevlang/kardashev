//SPEC: §2/§28.1 `and` binds tighter than `or` — a or b and c is a or (b and c)
//OUT: 1
//OUT: 1
pub fn main() void {
    var t: bool = 1 < 2;  // true, computed
    var f: bool = 2 < 1;  // false, computed
    // t or (f and f) = true.  Wrong grouping (t or f) and f = false.
    if (t or f and f) {
        print(1);
    } else {
        print(0);
    }
    // (f and f) or t = true.  Wrong grouping f and (f or t) = false.
    if (f and f or t) {
        print(1);
    } else {
        print(0);
    }
}
