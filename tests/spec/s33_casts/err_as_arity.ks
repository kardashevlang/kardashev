//SPEC: §33 `@as` takes exactly two arguments (a type and a value) — one or three is an error
//ERR: E0320
pub fn main() void {
    var a: i64 = @as(i64);
    var b: i64 = @as(i64, 1, 2);
}
