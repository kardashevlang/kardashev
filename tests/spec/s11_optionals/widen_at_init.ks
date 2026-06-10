//SPEC: §11.2 a `T` value widens to `?T` at an init site
//OUT: 42
pub fn main() void {
    var x: ?i64 = 42;
    print(x orelse 0);
}
