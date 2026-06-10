//SPEC: §44.1 `@arg`'s index must be an integer
//ERR: E0110

pub fn main() void {
    var a: Allocator = c_allocator();
    var s: []u8 = @arg(a, "zero");
    print(s.len);
}
