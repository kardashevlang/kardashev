//SPEC: §27.3 `/=` and `%=` through index places write the element in place — on the array and through a slice view
//OUT: 14
//OUT: 7
//OUT: 4
//OUT: 4

pub fn main() void {
    var a: [3]i64 = [3]i64{ 100, 47, 8 };
    a[0] /= 7;            // 100 / 7 = 14
    a[1] %= 10;           // 47 % 10 = 7
    print(a[0]);          // 14
    print(a[1]);          // 7
    var s: []i64 = a[0..3];
    s[2] /= 2;            // 8 / 2 = 4, through the slice view
    print(a[2]);          // 4 — visible on the backing array
    var i: usize = 0;
    s[i] %= 5;            // 14 % 5 = 4, a variable index
    print(a[0]);          // 4
}
