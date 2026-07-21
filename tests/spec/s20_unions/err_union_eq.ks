//SPEC: §3 x §20 tagged-union values do not support `==` comparison
//ERR: E0110

const U = union(enum) { a: i64 };

pub fn main() void {
    var x: U = U{ .a = 1 };
    var y: U = U{ .a = 1 };
    if (x == y) { print(1); }
}
