//SPEC: §31.1 each instance is interned as `<Ctor>__<tag1>_<tag2>…` — both orderings of `Map` carry the `Map__` prefix yet name DIFFERENT structs
//OUT: 1
//OUT: 1
//OUT: 1

@import("std");

fn Map(comptime K: type, comptime V: type) type {
    return struct {
        k: K,
        v: V,

        fn myname(self: Self) []u8 {
            return @typeName(Self);   // the interned instance name (§32.1)
        }
    };
}

const A = Map(u8, i64);
const B = Map(i64, u8);

pub fn main() void {
    var a: A = A{ .k = 1, .v = 1 };
    var b: B = B{ .k = 1, .v = 1 };
    if (str_starts_with(a.myname(), "Map__")) { print(1); } else { print(0); }
    if (str_starts_with(b.myname(), "Map__")) { print(1); } else { print(0); }
    // Distinct tuples ⇒ distinct interned names (the exact tag spelling is
    // unspecified, so only the documented prefix + distinctness are pinned).
    if (str_eq(a.myname(), b.myname())) { print(0); } else { print(1); }
}
