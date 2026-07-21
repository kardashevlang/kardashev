//SPEC: §41 `@readFile` of an empty path fails to open and yields the empty slice
//OUT: 0

pub fn main() void {
    var a: Allocator = c_allocator();
    var d: []u8 = @readFile(a, "");
    print(d.len);
    free(a, d);
}
