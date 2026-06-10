//SPEC: §14.2 the index in `a[i]` must be an integer
//ERR: E0110

pub fn main() void {
    var a: [2]i64 = [2]i64{ 1, 2 };
    print(a[true]);     // a bool cannot index
}
