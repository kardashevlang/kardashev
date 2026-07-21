//SPEC: §11 x §20 `?Union`: a union value widens into the optional at a call argument, `if |v|` unwraps it into a switch, null takes the else
//OUT: 21
//OUT: 0

const Val = union(enum) { n: i64, s: []u8 };

fn describe(ov: ?Val) void {
    if (ov) |v| {
        switch (v) {
            .n => |x| { print(x); },
            .s => |t| { print(t.len); },
        }
    } else {
        print(0);
    }
}

pub fn main() void {
    describe(Val{ .n = 21 });   // T → ?T widening at the call argument
    describe(null);
}
