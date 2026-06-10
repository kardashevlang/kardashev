//SPEC: §33 `@as`'s first argument must name a numeric type — `bool` and struct targets are rejected
//ERR: E0321
const P = struct {
    x: i64,
};

pub fn main() void {
    var a: bool = @as(bool, 1);
    var b: P = @as(P, 5);
}
