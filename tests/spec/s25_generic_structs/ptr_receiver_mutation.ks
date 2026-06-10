//SPEC: §26.1/§30 a generic-struct method may take `self: *Self` — the call auto-references the receiver variable and mutates it in place
//OUT: 0
//OUT: 7
//OUT: 17

fn Counter(comptime T: type) type {
    return struct {
        n: T,

        fn bump(self: *Self, by: T) void {
            self.n = self.n + by; // true mutation through the pointer receiver
        }

        fn get(self: Self) T {
            return self.n;
        }
    };
}

const C = Counter(i64);

pub fn main() void {
    var c: C = C{ .n = 0 };
    print(c.get()); // 0
    c.bump(7);
    print(c.get()); // 7  — the first bump persisted
    c.bump(10);
    print(c.n); // 17 — and they accumulate
}
