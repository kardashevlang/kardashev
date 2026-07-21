//SPEC: §16.1 `alloc`'s literal type argument does not accept a type ALIAS (the named-type-only rule)
//ERR: E0241

fn Box(comptime T: type) type {
    return struct { v: T };
}

const IntBox = Box(i32);

pub fn main() void {
    var al: Allocator = c_allocator();
    var s: []IntBox = alloc(al, IntBox, 2);
    free(al, s);
}
