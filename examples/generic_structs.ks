// generic_structs.ks — generic structs via type-returning functions (v0.129).
//
// Zig's metaprogramming for types: a function whose return type is `type` and
// whose body is `return struct { ... };` is a *type-constructor*. Instantiate it
// at a concrete type with a `const` type alias, then use the alias as a type.

fn Pair(comptime T: type) type {
    return struct { first: T, second: T };
}

// Bind concrete instantiations as type aliases (memoised: same args → same type).
const IntPair = Pair(i32);
const BigPair = Pair(i64);

fn sum(p: IntPair) i32 {
    return p.first + p.second;
}

fn swap(p: IntPair) IntPair {
    return IntPair{ .first = p.second, .second = p.first };
}

pub fn main() i32 {
    var p: IntPair = IntPair{ .first = 10, .second = 32 };
    print(sum(p));                 // 42
    print(p.first);                // 10

    var s: IntPair = swap(p);
    print(s.first);                // 32  (swapped)

    // A distinct instantiation is a distinct type with i64 fields.
    var big: BigPair = BigPair{ .first = 1000000, .second = 337 };
    print(big.first + big.second); // 1000337

    // Inferred binding of an instantiated struct literal.
    var q = IntPair{ .first = 3, .second = 4 };
    print(sum(q));                 // 7
    return 0;
}

test "generic pair" {
    var p: IntPair = IntPair{ .first = 5, .second = 6 };
    expect(sum(p) == 11);
    expect(swap(p).first == 6);
    var big: BigPair = BigPair{ .first = 40, .second = 2 };
    expect(big.first + big.second == 42);
}
