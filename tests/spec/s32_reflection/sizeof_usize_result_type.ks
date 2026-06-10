//SPEC: §32.1 `@sizeOf(T)` has type `usize` (assigns and mixes with `usize` like `s.len`, no cast), is positive, and accepts builtin / struct / enum type names
//OUT: 1
//OUT: 1
//OUT: 1
//OUT: 1
const P = struct {
    a: i64,
    b: i64,
};

const Color = enum {
    Red,
    Green,
};

pub fn main() void {
    var n: usize = @sizeOf(i64);     // usize result: no cast needed
    var arr: [4]u8 = [4]u8{ 1, 2, 3, 4 };
    var s: []u8 = arr[0..4];
    var total: usize = n + s.len;    // usize + usize (s.len is usize, §15.2)
    if (total > s.len) { print(1); } else { print(0); }   // so @sizeOf(i64) > 0
    if (@sizeOf(P) > 0) { print(1); } else { print(0); }  // a struct name is a valid argument
    if (@sizeOf(Color) > 0) { print(1); } else { print(0); }  // an enum name too (§16's type-arg set)
    var e: usize = @sizeOf(bool);
    if (e > 0) { print(1); } else { print(0); }
}
