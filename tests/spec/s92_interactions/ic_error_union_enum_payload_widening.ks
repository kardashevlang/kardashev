//SPEC: §12.2 x §13 a contextual `.Variant` widens into `!Enum` at a return site; catch supplies a contextual default
//OUT: 1
//OUT: 2

const Color = enum { Red, Green, Blue };

fn code(c: Color) i64 {
    switch (c) {
        .Red => { return 0; },
        .Green => { return 1; },
        .Blue => { return 2; },
    }
}

fn pick(ok: bool) !Color {
    if (ok) { return .Green; }   // the contextual literal widens into !Color
    return error.Nope;
}

pub fn main() void {
    print(code(pick(true) catch .Red));     // success payload: Green
    print(code(pick(false) catch .Blue));   // error path: the contextual default
}
