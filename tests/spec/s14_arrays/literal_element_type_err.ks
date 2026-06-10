//SPEC: §14.2 every array literal element must coerce to the element type
//ERR: E0110

pub fn main() void {
    var a: [2]i64 = [2]i64{ 1, true };    // bool is not an i64
    print(a[0]);
}
