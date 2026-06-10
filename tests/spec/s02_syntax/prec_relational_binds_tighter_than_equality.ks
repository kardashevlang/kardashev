//SPEC: §28.1 relational (< <= > >=) binds tighter than equality (== !=) — t == 1 < 2 is t == (1 < 2)
//OUT: 1
//OUT: 0
pub fn main() void {
    var t: bool = 0 < 1; // true, computed
    var f: bool = 1 < 0; // false, computed
    // t == (1 < 2) = true == true.  Wrong grouping (t == 1) < 2: type error.
    if (t == 1 < 2) {
        print(1);
    } else {
        print(0);
    }
    // f != (2 < 1) = false != false = false.
    if (f != 2 < 1) {
        print(1);
    } else {
        print(0);
    }
}
