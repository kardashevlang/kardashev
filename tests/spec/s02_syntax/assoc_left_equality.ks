//SPEC: §2 comparison operators are left-associative — 1 == 1 == true is (1 == 1) == true (right grouping would be int-vs-bool and not compile)
//OUT: 1
//OUT: 1
pub fn main() void {
    var one: i64 = 1;
    if (one == 1 == true) {
        print(1);
    } else {
        print(0);
    }
    // (2 != 2) == false  ->  false == false  ->  true.
    if (2 != 2 == false) {
        print(1);
    } else {
        print(0);
    }
}
