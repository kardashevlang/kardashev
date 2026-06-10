//SPEC: §37.1 `@intFromEnum`'s argument must be an enum value — a non-enum is E0321
//ERR: E0321

const Color = enum { Red, Green, Blue };

pub fn main() void {
    print(@intFromEnum(5));   // an i64, not an enum
}
