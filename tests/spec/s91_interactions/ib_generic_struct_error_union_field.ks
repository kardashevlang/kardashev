//SPEC: §26×§12.2 a generic struct may STORE an error union of its type param (`pending: !V`) — error literals and values coerce at field-init and field-assign
//OUT: -4
//OUT: 12

fn Box(comptime V: type) type {
    return struct {
        pending: !V,
    };
}

const B = Box(i64);

pub fn main() void {
    // `error.Nope` coerces to the !i64 FIELD at struct-literal init.
    var b: B = B{ .pending = error.Nope };
    var v: i64 = b.pending catch 0 - 4;
    print(v);                       // -4 — the stored state is the error

    // A bare i64 coerces to the same field on assignment.
    b.pending = 12;
    print(b.pending catch 0 - 4);   // 12 — now the success payload
}
