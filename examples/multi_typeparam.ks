// multi_typeparam.ks — generics over more than one type (v0.135).
//
// Generic functions already take several comptime parameters; v0.135 extends
// the same to type-constructors: `fn Map(comptime K: type, comptime V: type)
// type { return struct { … }; }`. Instantiate with a `const` type alias and the
// argument order matters — `Pair(i32, i64)` and `Pair(i64, i32)` are distinct.

// A generic function over two element types: pick the first.
fn first_of(comptime A: type, comptime B: type, a: A, b: B) A {
    return a;
}

// A two-parameter container with methods that use both type parameters.
fn Pair(comptime A: type, comptime B: type) type {
    return struct {
        first: A,
        second: B,
        fn set(self: *Self, a: A, b: B) void {
            self.first = a;
            self.second = b;
        }
        fn fst(self: Self) A { return self.first; }
        fn snd(self: Self) B { return self.second; }
    };
}

const Entry = Pair(i32, i64);   // key: i32, value: i64

pub fn main() i32 {
    print(first_of(i32, i64, 42, 999));   // 42 (generic fn, 2 type args)

    var e: Entry = Entry{ .first = 1, .second = 1000 };
    print(e.fst());                        // 1
    print(e.snd());                        // 1000
    e.set(7, 70000);                       // mutate (both type params)
    print(e.fst());                        // 7
    print(e.snd());                        // 70000
    return 0;
}

test "multiple type parameters" {
    var e: Entry = Entry{ .first = 3, .second = 4 };
    expect(e.fst() == 3);
    expect(e.snd() == 4);
    e.set(10, 20);
    expect(e.fst() == 10);
    expect(e.snd() == 20);
    expect(first_of(i64, i32, 5, 6) == 5);
}
