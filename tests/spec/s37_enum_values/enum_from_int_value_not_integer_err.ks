//SPEC: §37.1 `@enumFromInt`'s second argument must be an integer — a bool is E0321
//ERR: E0321

const Color = enum { Red, Green, Blue };

pub fn main() void {
    var c: Color = @enumFromInt(Color, true);
    print(0);
}
