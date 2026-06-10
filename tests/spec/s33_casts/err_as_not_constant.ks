//SPEC: §33 `@as` is not a constant expression — a top-level `const` initializer cannot fold it
//ERR: E0130
const K = @as(i64, 3);

pub fn main() void {
    print(1);
}
