//SPEC: §32.1 `@sizeOf(T)` inside a generic body resolves through the substitution — each instance reports its own bound type's size
//OUT: 1
//OUT: 1
//OUT: 1
fn sz(comptime T: type) usize {
    return @sizeOf(T);
}

pub fn main() void {
    // The generic answer must equal the direct answer for the same type...
    if (sz(i64) == @sizeOf(i64)) { print(1); } else { print(0); }
    if (sz(u8) == @sizeOf(u8)) { print(1); } else { print(0); }
    // ...and the two instances must DISAGREE with each other (i64 is a wider
    // C type than u8, so their sizeof differ — collapsing the instances into
    // one would make these equal).
    if (sz(u8) == sz(i64)) { print(0); } else { print(1); }
}
