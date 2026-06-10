// tests/std/bitset.ks — std BitSet test suite (v0.154 std wave 1).

@import("std");

test "set/has/clear/toggle across word boundaries" {
    var a: Allocator = c_allocator();
    var b: BitSet = BitSet.init(a, 130);
    expect(b.capacity() == 130);
    expect(b.is_empty());
    expect(b.count() == 0);

    b.set(0);
    b.set(63);
    b.set(64);
    b.set(65);
    b.set(127);
    expect(b.has(0));
    expect(b.has(63));
    expect(b.has(64));
    expect(b.has(65));
    expect(b.has(127));
    expect(!b.has(1));
    expect(!b.has(62));
    expect(!b.has(66));
    expect(!b.has(126));
    expect(b.count() == 5);
    expect(!b.is_empty());

    b.clear(64);
    expect(!b.has(64));
    expect(b.count() == 4);

    b.toggle(64); // back on -> {0, 63, 64, 65, 127}
    expect(b.has(64));
    b.toggle(63); // off     -> {0, 64, 65, 127}
    expect(!b.has(63));
    expect(b.count() == 4);

    b.deinit(a);
}

test "single-bit set" {
    var a: Allocator = c_allocator();
    var b: BitSet = BitSet.init(a, 1);
    expect(b.capacity() == 1);
    expect(b.is_empty());
    b.set(0);
    expect(b.has(0));
    expect(b.count() == 1);
    b.toggle(0);
    expect(!b.has(0));
    expect(b.is_empty());
    b.deinit(a);
}

test "empty set (nbits == 0)" {
    var a: Allocator = c_allocator();
    var b: BitSet = BitSet.init(a, 0);
    expect(b.capacity() == 0);
    expect(b.count() == 0);
    expect(b.is_empty());
    expect(!b.has(0));
    b.set(0); // guarded no-op
    expect(b.is_empty());
    b.deinit(a);
}

test "out-of-range bit indexes are guarded no-ops" {
    var a: Allocator = c_allocator();
    var b: BitSet = BitSet.init(a, 10);
    expect(!b.has(10));
    expect(!b.has(1000));
    b.set(10);     // == nbits: no-op
    b.set(64);     // beyond the single backing word: no-op
    expect(b.count() == 0);
    b.toggle(10);  // no-op
    expect(b.is_empty());
    b.set(9);
    b.clear(10);   // no-op
    expect(b.count() == 1);
    expect(b.has(9));
    b.deinit(a);
}

test "count on a full word + raw word pin" {
    var a: Allocator = c_allocator();
    var b: BitSet = BitSet.init(a, 64);
    var i: usize = 0;
    while (i < 64) : (i += 1) {
        b.set(i);
    }
    expect(b.count() == 64);
    expect(b.words[0] == ~@as(u64, 0)); // all 64 bits set
    b.clear(63);
    expect(b.count() == 63);
    expect(b.words[0] == 9223372036854775807); // 2^63 - 1
    b.deinit(a);
}

test "union/intersect/difference on hand-computed masks" {
    var a: Allocator = c_allocator();
    var x: BitSet = BitSet.init(a, 128);
    var y: BitSet = BitSet.init(a, 128);
    // x = {1, 5, 64, 100}, y = {5, 64, 99}
    x.set(1);
    x.set(5);
    x.set(64);
    x.set(100);
    y.set(5);
    y.set(64);
    y.set(99);
    expect(x.words[0] == 34);          // 2^1 + 2^5
    expect(x.words[1] == 68719476737); // 2^0 + 2^36 (bits 64 and 100)

    var u: BitSet = BitSet.init(a, 128);
    u.union_with(x); // u = x
    u.union_with(y); // u = x | y = {1, 5, 64, 99, 100}
    expect(u.count() == 5);
    expect(u.has(1));
    expect(u.has(5));
    expect(u.has(64));
    expect(u.has(99));
    expect(u.has(100));
    expect(!u.has(0));
    expect(!u.has(63));

    var n: BitSet = BitSet.init(a, 128);
    n.union_with(x);
    n.intersect_with(y); // x & y = {5, 64}
    expect(n.count() == 2);
    expect(n.has(5));
    expect(n.has(64));
    expect(!n.has(1));
    expect(!n.has(99));
    expect(!n.has(100));

    var d: BitSet = BitSet.init(a, 128);
    d.union_with(x);
    d.difference_with(y); // x & ~y = {1, 100}
    expect(d.count() == 2);
    expect(d.has(1));
    expect(d.has(100));
    expect(!d.has(5));
    expect(!d.has(64));

    d.deinit(a);
    n.deinit(a);
    u.deinit(a);
    y.deinit(a);
    x.deinit(a);
}

test "clear_all resets every bit but keeps capacity" {
    var a: Allocator = c_allocator();
    var b: BitSet = BitSet.init(a, 200);
    var i: usize = 0;
    while (i < 200) : (i += 1) {
        b.set(i);
    }
    expect(b.count() == 200);
    b.clear_all();
    expect(b.count() == 0);
    expect(b.is_empty());
    expect(b.capacity() == 200);
    expect(!b.has(0));
    expect(!b.has(199));
    b.deinit(a);
}

test "property: every-7th-bit pattern in 1000 bits" {
    var a: Allocator = c_allocator();
    var b: BitSet = BitSet.init(a, 1000);
    var i: usize = 0;
    while (i < 1000) : (i += 7) {
        b.set(i);
    }
    expect(b.count() == 143); // ceil(1000 / 7): bits 0, 7, ..., 994

    // Membership matches `j % 7 == 0` for every index in range.
    var ok: bool = true;
    var j: usize = 0;
    while (j < 1000) : (j += 1) {
        if (b.has(j) != (j % 7 == 0)) {
            ok = false;
        }
    }
    expect(ok);

    // (C | A) & A == A, then (C | A) & ~A == 0.
    var c: BitSet = BitSet.init(a, 1000);
    c.union_with(b);       // c = b
    expect(c.count() == 143);
    c.intersect_with(b);   // unchanged
    expect(c.count() == 143);
    c.difference_with(b);  // emptied
    expect(c.count() == 0);
    expect(c.is_empty());

    // Toggling every set bit empties the original too.
    var k: usize = 0;
    while (k < 1000) : (k += 7) {
        b.toggle(k);
    }
    expect(b.is_empty());

    c.deinit(a);
    b.deinit(a);
}
