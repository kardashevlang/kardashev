//SPEC: §3 comparison operators yield `bool`; `==`/`!=` also compare two `bool`s
//OUT: 11
fn xor(a: bool, b: bool) bool {
    return a != b; // bool != bool — same-type comparison on bools
}
fn abs(v: i64) i64 {
    if (v < 0) {
        return -v;
    }
    return v;
}
pub fn main() void {
    var n: i64 = 0;
    if (xor(abs(-3) == 3, 2 > 5)) {
        n = n + 1; // true xor false → taken
    }
    if ((1 < 2) == (10 >= 10)) {
        n = n + 10; // (bool) == (bool): true == true → taken
    }
    if (xor(4 <= 3, 7 != 7)) {
        n = n + 100; // false xor false → not taken
    }
    print(n); // 1 + 10 = 11
}
