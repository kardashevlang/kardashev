//SPEC: §14.2 index-assignment through an immutable `const` binding is rejected
//ERR: E0223

pub fn main() void {
    const a = [2]i64{ 1, 2 };    // an immutable local binding
    a[0] = 5;
    print(a[0]);
}
