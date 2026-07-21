//SPEC: §3 x §20 `print` rejects a tagged-union argument
//ERR: E0110

const U = union(enum) { a: i64 };

pub fn main() void {
    var x: U = U{ .a = 1 };
    print(x);
}
