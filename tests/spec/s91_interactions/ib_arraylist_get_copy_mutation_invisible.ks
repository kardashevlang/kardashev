//SPEC: std ArrayList×§9×§30 `get` hands out a struct COPY — a pointer-receiver mutation of the copy never reaches the element; `set` writes it back
//OUT: 11
//OUT: 1
//OUT: 21

@import("std");

const Counter = struct {
    n: i64,
    fn bump(self: *Counter) void {
        self.n += 10;
    }
};

const L = ArrayList(Counter);

pub fn main() void {
    var a: Allocator = c_allocator();
    var l: L = L.init(a);
    l.push(a, Counter{ .n = 1 });

    var c: Counter = l.get(0);   // a value copy of the element
    c.bump();                    // auto-ref targets the COPY
    print(c.n);                  // 11

    var probe: Counter = l.get(0);
    print(probe.n);              // 1 — the stored element never moved

    c.bump();                    // copy -> 21
    l.set(0, c);                 // explicit write-back
    var after: Counter = l.get(0);
    print(after.n);              // 21
    l.deinit(a);
}
