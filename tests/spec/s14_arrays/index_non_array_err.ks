//SPEC: §14.2 indexing a value that is neither an array nor a slice is rejected
//ERR: E0220

pub fn main() void {
    var x: i64 = 42;
    print(x[0]);      // an i64 has no elements
}
