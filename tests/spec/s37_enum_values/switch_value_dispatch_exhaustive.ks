//SPEC: §37.2 `switch` over an explicit-valued enum dispatches by VALUE — an `@enumFromInt`-built scrutinee reaches the right arm; exhaustiveness stays variant-based (no else needed)
//OUT: 2
//OUT: 3
//OUT: 1

const Status = enum { Ok = 200, NotFound = 404, Teapot = 418 };

fn classify(s: Status) i64 {
    switch (s) {
        .Ok => { return 1; },
        .NotFound => { return 2; },
        .Teapot => { return 3; },
    }
}

pub fn main() void {
    // Were dispatch ordinal-based (0,1,2), these values could not land.
    print(classify(@enumFromInt(Status, 404)));
    print(classify(@enumFromInt(Status, 418)));
    print(classify(.Ok));
}
