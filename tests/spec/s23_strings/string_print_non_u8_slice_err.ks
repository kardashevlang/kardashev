//SPEC: §23.1 `print` accepts `[]u8` but no other slice — a `[]i64` argument stays E0110
//ERR: E0110

pub fn main() void {
    var a: [2]i64 = [2]i64{ 1, 2 };
    var s: []i64 = a[0..2];
    print(s);
}
