//SPEC: §26.1 an associated function (no `self`) on a generic struct builds `Self{ … }` and is called as `Alias.assoc(args)`
//OUT: 9
//OUT: 16

fn Sq(comptime T: type) type {
    return struct {
        v: T,

        fn of(x: T) Self {
            return Self{ .v = x * x }; // `Self` literal names the instantiated struct
        }
    };
}

const S = Sq(i64);

pub fn main() void {
    var s: S = S.of(3);
    print(s.v); // 9
    var t: S = S.of(4);
    print(t.v); // 16
}
