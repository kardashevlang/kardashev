//SPEC: §28.1 shift (<< >>) binds tighter than relational — 1 < 1 << 1 is 1 < (1 << 1)
//OUT: 1
//OUT: 0
pub fn main() void {
    var one: i64 = 1;
    // 1 < (1 << 1) = 1 < 2 = true.  Wrong grouping (1 < 1) << 1: type error.
    if (one < one << 1) {
        print(1);
    } else {
        print(0);
    }
    // (4 >> 1) >= 3 = 2 >= 3 = false.
    if (4 >> one >= 3) {
        print(1);
    } else {
        print(0);
    }
}
