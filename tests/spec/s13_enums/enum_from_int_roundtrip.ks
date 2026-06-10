//SPEC: §37 `@enumFromInt(E, n)` yields the variant valued `n`; `@intFromEnum` round-trips it
//OUT: 4
//OUT: 2
//OUT: 2

const Dir = enum { North, East, South, West };   // values 0,1,2,3

// Rotate by integer arithmetic through the int<->enum conversions.
fn turn(d: Dir, steps: i64) Dir {
    return @enumFromInt(Dir, (@intFromEnum(d) + steps) % 4);
}

fn code(d: Dir) i64 {
    switch (d) {
        .North => { return 1; },
        .East => { return 2; },
        .South => { return 3; },
        .West => { return 4; },
    }
}

pub fn main() void {
    var d: Dir = .North;
    d = turn(d, 3);                       // (0+3)%4 = 3 -> West
    print(code(d));                       // a converted value drives `switch`
    d = turn(d, 2);                       // (3+2)%4 = 1 -> East
    print(code(d));
    print(@intFromEnum(turn(d, 1)));      // (1+1)%4 = 2 -> South -> 2
}
