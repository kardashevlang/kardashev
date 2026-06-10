//SPEC: §42.4 composite type ARGUMENTS stay deferred — `Box([]u8)` fails to parse (the argument must be a name, E0200)
//ERR: E0200

fn Box(comptime T: type) type {
    return struct {
        v: T,
    };
}

pub fn main() void {
    var b: Box([]u8) = undefined;
    print(1);
}
