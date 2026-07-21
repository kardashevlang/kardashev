//SPEC: §26 x §20 a generic struct instantiated at a union: the field holds the union, alias and application share the instance
//OUT: 8
//OUT: 3

const Msg = union(enum) { code: i64, flag: bool };

fn Box(comptime T: type) type {
    return struct {
        v: T,
        fn get(self: Self) T { return self.v; }
    };
}

const MsgBox = Box(Msg);

pub fn main() void {
    var b: MsgBox = MsgBox{ .v = Msg{ .code = 8 } };
    switch (b.get()) {
        .code => |c| { print(c); },
        .flag => { print(0); },
    }
    // The direct application names the SAME instance as the alias (§42.2).
    var b2: Box(Msg) = MsgBox{ .v = Msg{ .flag = true } };
    switch (b2.get()) {
        .code => { print(0); },
        .flag => |f| {
            if (f) { print(3); }
        },
    }
}
