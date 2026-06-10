//SPEC: §41.1 `@readFile`'s path must be a `[]u8`
//ERR: E0110

pub fn main() void {
    var a: Allocator = c_allocator();
    var s: []u8 = @readFile(a, 42);   // an integer is not a path
    print(s.len);
}
