//SPEC: §32.2 `@This()` works inside a generic struct's methods — interchangeable with `Self` for value and pointer receivers
//OUT: 42
//OUT: 255
fn Cell(comptime T: type) type {
    return struct {
        v: T,

        fn put(self: *@This(), x: T) void {   // pointer receiver via @This()
            self.v = x;
        }

        fn get(self: @This()) T {             // value receiver via @This()
            return self.v;
        }

        fn add(self: *Self, x: T) void {      // the same struct, spelled Self
            self.v += x;
        }
    };
}

const CI = Cell(i64);
const CU = Cell(u8);

pub fn main() void {
    var c: CI = CI{ .v = 1 };
    c.put(40);
    c.add(2);
    print(c.get());    // 42

    var u: CU = CU{ .v = 250 };
    u.add(5);
    print(u.get());    // 255 — @This()/Self resolve per instance
}
