//SPEC: §32.1 `@typeName(T)` is a `[]u8` holding the SOURCE name of `T` — exact strings for every builtin scalar
//OUT: i8
//OUT: i16
//OUT: i32
//OUT: i64
//OUT: u8
//OUT: u16
//OUT: u32
//OUT: u64
//OUT: usize
//OUT: bool
pub fn main() void {
    print(@typeName(i8));
    print(@typeName(i16));
    print(@typeName(i32));
    print(@typeName(i64));
    print(@typeName(u8));
    print(@typeName(u16));
    print(@typeName(u32));
    print(@typeName(u64));
    print(@typeName(usize));
    print(@typeName(bool));
}
