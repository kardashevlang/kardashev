//SPEC: §37.1 a wrong argument count to `@intFromEnum` / `@enumFromInt` is E0320 (the @-builtin arity code)
//ERR: E0320

const Color = enum { Red, Green, Blue };

pub fn main() void {
    var c: Color = .Red;
    print(@intFromEnum(c, 1));        // takes exactly 1 argument
    var d: Color = @enumFromInt(Color);   // takes exactly 2
    print(0);
}
