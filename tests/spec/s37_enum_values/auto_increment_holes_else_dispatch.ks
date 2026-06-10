//SPEC: §37.1 auto-incremented variants between explicit anchors carry their resolved values in `switch` dispatch; `else` catches the uncovered variant
//OUT: 2
//OUT: 3
//OUT: 99
//OUT: 12

const Code = enum { A = 5, B, C = 11, D };   // B = 6 (auto), D = 12 (auto)

fn route(c: Code) i64 {
    switch (c) {
        .A => { return 1; },
        .B => { return 2; },
        .D => { return 3; },
        else => { return 99; },   // covers .C
    }
}

pub fn main() void {
    print(route(@enumFromInt(Code, 6)));    // auto value 6 -> .B
    print(route(@enumFromInt(Code, 12)));   // auto value 12 -> .D
    print(route(.C));                       // unlisted variant -> else
    print(@intFromEnum(Code.D));            // 12
}
