//SPEC: §33 `@as`'s second argument must be a numeric VALUE — a bool or a string is rejected
//ERR: E0321
pub fn main() void {
    var a: i32 = @as(i32, true);
    var b: i64 = @as(i64, "hi");
}
