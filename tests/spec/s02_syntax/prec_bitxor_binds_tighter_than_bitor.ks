//SPEC: §28.1 `^` binds tighter than `|` — a | b ^ c is a | (b ^ c)
//OUT: 1
//OUT: 4
pub fn main() void {
    var a: i64 = 1;
    var b: i64 = 2;
    var c: i64 = 3;
    // 1 | (2 ^ 3) = 1 | 1 = 1.  Wrong grouping (1 | 2) ^ 3 = 0.
    print(a | b ^ c);
    // 4 | (6 ^ 6) = 4 | 0 = 4.  Wrong grouping (4 | 6) ^ 6 = 0.
    print(4 | 6 ^ 6);
}
