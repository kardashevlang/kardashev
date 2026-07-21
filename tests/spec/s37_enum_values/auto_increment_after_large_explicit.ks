//SPEC: §37 auto-increment continues from a large explicit value; a later explicit value may go back down
//OUT: 1000
//OUT: 1001
//OUT: 5

const E = enum { A = 1000, B, C = 5 };

pub fn main() void {
    print(@intFromEnum(E.A));
    print(@intFromEnum(E.B));   // 1000 + 1
    print(@intFromEnum(E.C));   // explicit again
}
