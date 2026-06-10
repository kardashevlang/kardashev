//SPEC: §21.2 a `return error.X` flushes defers AND errdefers, merged in reverse registration order
//OUT: 3
//OUT: 2
//OUT: 1
//OUT: -1

fn compute(flag: bool) !i64 {
    defer print(1);             // registered first -> flushes last
    errdefer print(2);          // fires on this path, in its LIFO slot
    defer print(3);             // registered last -> flushes first
    if (flag) {
        return error.Nope;      // error-return edge: 3, 2, 1
    }
    return 42;
}

pub fn main() void {
    print(compute(true) catch 0 - 1);   // -1 after the merged flush
}
