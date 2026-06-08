// counter.ks — struct methods + associated functions (v0.113).
//
// A method's first parameter is `self`; an associated function has no `self`.
// Call methods as `instance.method(args)` and associated functions as
// `Type.func(args)`. The explicit-self form `Type.method(instance, args)` works
// too. Method calls chain: `c.bumped(1).bumped(2)`.

const Counter = struct {
    n: i32,

    pub fn zero() Counter {
        return Counter{ .n = 0 };
    }

    pub fn get(self: Counter) i32 {
        return self.n;
    }

    pub fn bumped(self: Counter, by: i32) Counter {
        return Counter{ .n = self.n + by };
    }
};

pub fn main() i32 {
    var c: Counter = Counter.zero();
    print(c.get());                          // 0
    c = c.bumped(5);
    print(c.get());                          // 5
    print(c.bumped(10).bumped(100).get());   // 115 (chained)
    print(Counter.get(c));                   // 5  (explicit-self form)
    return 0;
}

test "counter methods" {
    var c: Counter = Counter.zero();
    expect(c.get() == 0);
    expect(c.bumped(7).get() == 7);
    expect(c.bumped(3).bumped(4).get() == 7);
}
