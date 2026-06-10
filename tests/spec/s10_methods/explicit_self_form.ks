//SPEC: §10 `Type.method(value, args)` is the explicit-self form of `value.method(args)`
//OUT: 42
//OUT: 42
//OUT: 0
const C = struct {
    n: i32,

    fn plus(self: C, k: i32) i32 {
        return self.n + k;
    }
};

pub fn main() void {
    var c: C = C{ .n = 30 };
    print(c.plus(12));            // 42 — receiver form
    print(C.plus(c, 12));         // 42 — explicit-self form, same resolution
    print(C.plus(c, 5) - c.plus(5));   // 0 — the two forms always agree
}
