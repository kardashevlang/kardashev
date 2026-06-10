//SPEC: §42.4 the struct-literal application `Name(T){ .f = v }` stays deferred — it does not parse (E0200)
//ERR: E0200

fn Box(comptime T: type) type {
    return struct {
        v: T,
    };
}

pub fn main() void {
    var b: Box(i32) = Box(i32){ .v = 3 };   // use an alias or an assoc init
    print(b.v);
}
