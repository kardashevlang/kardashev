//SPEC: §14.2 an array literal's element count must equal its declared `N`
//ERR: E0221

pub fn main() void {
    var a: [3]i64 = [3]i64{ 1, 2 };    // 2 elements for a [3]i64
    print(a[0]);
}
