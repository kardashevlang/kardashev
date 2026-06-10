//SPEC: §13.2 `Enum.V` where `V` is not a declared variant is rejected
//ERR: E0212

const Color = enum { Red, Green };

pub fn main() void {
    var c: Color = Color.Purple;
    print(@intFromEnum(c));
}
