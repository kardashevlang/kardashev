//SPEC: §32.1 `@sizeOf` accepts a type ALIAS: the monomorphised single-field struct is its field's size
//OUT: 4
//OUT: IntBox

fn Box(comptime T: type) type {
    return struct { v: T };
}

const IntBox = Box(i32);

pub fn main() void {
    print(@sizeOf(IntBox));       // struct { int32_t } is 4 bytes
    print(@typeName(IntBox));     // an unbound name prints as written
}
