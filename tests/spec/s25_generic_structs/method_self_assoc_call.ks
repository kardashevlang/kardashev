//SPEC: §26.2 a method body may call `Self.assoc(…)` — the `Self` receiver resolves through the active substitution (v0.138)
//OUT: 21

fn Pt(comptime T: type) type {
    return struct {
        x: T,

        fn make(v: T) Self {
            return Self{ .x = v };
        }

        fn shifted(self: Self, d: T) Self {
            return Self.make(self.x + d); // associated call THROUGH `Self`
        }
    };
}

const P = Pt(i64);

pub fn main() void {
    var p: P = P.make(6);
    var q: P = p.shifted(15);
    print(q.x); // 6 + 15 = 21
}
