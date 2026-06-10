//SPEC: §37.1 `@enumFromInt`'s first argument must NAME an enum type — a non-enum type is E0321
//ERR: E0321

pub fn main() void {
    var x: i64 = 3;
    print(@enumFromInt(i64, x));   // i64 is not an enum type
}
