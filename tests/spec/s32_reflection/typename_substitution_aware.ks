//SPEC: §32.1 the builtin's type argument is substitution-aware — `@typeName(T)` inside a generic body names the BOUND type, through forwarding too
//OUT: i32
//OUT: u16
//OUT: u8
fn tn(comptime T: type) []u8 {
    return @typeName(T);
}

fn outer(comptime T: type) []u8 {
    return tn(T);          // T forwards one generic level down
}

pub fn main() void {
    print(tn(i32));
    print(tn(u16));        // a second instance, a different answer
    print(outer(u8));
}
