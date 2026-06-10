//SPEC: §30 a generic struct's `self: *Self` method mutates the instance in place, per monomorphised instance (the type parameter drives the field type)
//OUT: 42
//OUT: 255
fn Box(comptime T: type) type {
    return struct {
        v: T,

        fn set(self: *Self, x: T) void {
            self.v = x;
        }

        fn add(self: *Self, x: T) void {
            self.v += x;       // compound write through the pointer receiver
        }
    };
}

const BI = Box(i64);
const BU = Box(u8);

pub fn main() void {
    var b: BI = BI{ .v = 1 };
    b.set(40);                 // auto-ref &b into the i64 instance
    b.add(2);
    print(b.v);                // 42

    var u: BU = BU{ .v = 250 };
    u.add(5);                  // the u8 instance mutates its own field type
    print(u.v);                // 255
}
