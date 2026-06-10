//SPEC: §28.1 `&` binds tighter than `^` — a ^ b & c is a ^ (b & c)
//OUT: 3
//OUT: 1
pub fn main() void {
    var a: i64 = 1;
    var b: i64 = 3;
    var c: i64 = 2;
    // 1 ^ (3 & 2) = 1 ^ 2 = 3.  Wrong grouping (1 ^ 3) & 2 = 2.
    print(a ^ b & c);
    // 5 ^ (4 & 6) = 5 ^ 4 = 1.  Wrong grouping (5 ^ 4) & 6 = 0.
    print(5 ^ 4 & 6);
}
