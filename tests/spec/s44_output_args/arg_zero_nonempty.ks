//SPEC: §44 `@arg(a, 0)` is argv[0] — a fresh, non-empty `[]u8` (the executable name always exists)
//OUT: 1

pub fn main() void {
    var a: Allocator = c_allocator();
    var exe: []u8 = @arg(a, 0);
    if (exe.len > 0) { print(1); } else { print(0); }
    free(a, exe);
}
