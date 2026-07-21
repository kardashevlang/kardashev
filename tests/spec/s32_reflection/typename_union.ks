//SPEC: §32.1 `@typeName` accepts a tagged-union name and yields it as written
//OUT: 5
//OUT: Shape

const Shape = union(enum) { n: i64 };

pub fn main() void {
    var name: []u8 = @typeName(Shape);
    print(name.len);
    print(name);
}
