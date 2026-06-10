//SPEC: §1 `1..3` lexes as integer `..` integer (a slice range), not as float literals
//OUT: 2
//OUT: 50
pub fn main() void {
    var a: [4]i64 = [4]i64{ 10, 20, 30, 40 };
    var s: []i64 = a[1..3];
    print(s.len);
    print(s[0] + s[1]);
}
