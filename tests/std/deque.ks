@import("std");

// tests/std/deque.ks — Deque(T), the growable ring-buffer double-ended queue.
// Every expectation below is a hand-computed value.

test "deque: starts empty, single element at either end" {
    var a: Allocator = c_allocator();
    var d: Deque(i32) = Deque(i32).init(a);
    expect(d.is_empty());
    expect(d.len() == 0);

    d.push_back(a, 7);
    expect(!d.is_empty());
    expect(d.len() == 1);
    expect(d.front() == 7);
    expect(d.back() == 7);
    expect(d.pop_front() == 7);
    expect(d.is_empty());

    // A lone push_front wraps head to the top of the ring (slot 3 of 4).
    d.push_front(a, 42);
    expect(d.len() == 1);
    expect(d.front() == 42);
    expect(d.back() == 42);
    expect(d.pop_back() == 42);
    expect(d.is_empty());
    expect(d.len() == 0);
    d.deinit(a);
}

test "deque: fifo via push_back / pop_front" {
    var a: Allocator = c_allocator();
    var d: Deque(i32) = Deque(i32).init(a);
    var i: i32 = 1;
    while (i <= 5) : (i += 1) {
        d.push_back(a, i);
    }
    expect(d.len() == 5);
    expect(d.front() == 1);
    expect(d.back() == 5);
    expect(d.pop_front() == 1);
    expect(d.pop_front() == 2);
    expect(d.pop_front() == 3);
    expect(d.pop_front() == 4);
    expect(d.pop_front() == 5);
    expect(d.is_empty());
    d.deinit(a);
}

test "deque: lifo via push_back / pop_back" {
    var a: Allocator = c_allocator();
    var d: Deque(i32) = Deque(i32).init(a);
    var i: i32 = 1;
    while (i <= 5) : (i += 1) {
        d.push_back(a, i);
    }
    expect(d.pop_back() == 5);
    expect(d.pop_back() == 4);
    expect(d.pop_back() == 3);
    expect(d.pop_back() == 2);
    expect(d.pop_back() == 1);
    expect(d.is_empty());
    d.deinit(a);
}

test "deque: push_front stacks at the front" {
    var a: Allocator = c_allocator();
    var d: Deque(i32) = Deque(i32).init(a);
    var i: i32 = 1;
    while (i <= 5) : (i += 1) {
        d.push_front(a, i);
    }
    // Front-to-back the deque is 5 4 3 2 1.
    expect(d.front() == 5);
    expect(d.back() == 1);
    expect(d.pop_front() == 5);
    expect(d.pop_front() == 4);
    expect(d.pop_back() == 1);
    expect(d.pop_back() == 2);
    expect(d.len() == 1);
    expect(d.front() == 3);
    expect(d.back() == 3);
    expect(d.pop_front() == 3);
    expect(d.is_empty());
    d.deinit(a);
}

test "deque: growth from minimal capacity keeps order" {
    var a: Allocator = c_allocator();
    var d: Deque(i32) = Deque(i32).init(a);
    // 21 appends force three doublings (cap 4 -> 8 -> 16 -> 32).
    var i: i32 = 0;
    while (i < 21) : (i += 1) {
        d.push_back(a, i);
    }
    expect(d.len() == 21);
    expect(d.front() == 0);
    expect(d.back() == 20);
    i = 0;
    while (i < 21) : (i += 1) {
        expect(d.pop_front() == i);
    }
    expect(d.is_empty());
    d.deinit(a);
}

test "deque: sustained wraparound without growth" {
    var a: Allocator = c_allocator();
    var d: Deque(i32) = Deque(i32).init(a);
    // Size never exceeds 1, so capacity stays 4 while head advances 50
    // slots — the ring wraps a dozen times.
    var i: i32 = 0;
    while (i < 50) : (i += 1) {
        d.push_back(a, i);
        expect(d.front() == i);
        expect(d.pop_front() == i);
    }
    expect(d.is_empty());
    expect(d.len() == 0);
    d.deinit(a);
}

test "deque: wraparound across two growths" {
    var a: Allocator = c_allocator();
    var d: Deque(i32) = Deque(i32).init(a);
    // Fill cap 4: ring is [0 1 2 3], head 0.
    d.push_back(a, 0);
    d.push_back(a, 1);
    d.push_back(a, 2);
    d.push_back(a, 3);
    expect(d.pop_front() == 0);   // head 1, count 3
    d.push_back(a, 4);            // wraps into slot 0: ring [4 1 2 3]
    expect(d.front() == 1);
    expect(d.back() == 4);
    d.push_back(a, 5);            // full + wrapped -> grow to 8, re-linearise
    expect(d.len() == 5);
    expect(d.front() == 1);
    expect(d.back() == 5);
    expect(d.pop_front() == 1);
    expect(d.pop_front() == 2);
    expect(d.pop_front() == 3);   // head 3, count 2 (elements 4 5)
    // Refill to cap 8; the last three land in slots 0 1 2 — wrapped again.
    var v: i32 = 6;
    while (v <= 11) : (v += 1) {
        d.push_back(a, v);
    }
    expect(d.len() == 8);
    expect(d.front() == 4);
    expect(d.back() == 11);
    d.push_back(a, 12);           // full + wrapped -> grow to 16
    expect(d.len() == 9);
    var w: i32 = 4;
    while (w <= 12) : (w += 1) {
        expect(d.pop_front() == w);
    }
    expect(d.is_empty());
    d.deinit(a);
}

test "deque: interleaved 100-op script" {
    var a: Allocator = c_allocator();
    var d: Deque(i32) = Deque(i32).init(a);

    // Phase A — 30 ops: push_back 0..29.  Deque: 0 1 .. 29.
    var i: i32 = 0;
    while (i < 30) : (i += 1) {
        d.push_back(a, i);
    }
    expect(d.len() == 30);
    expect(d.front() == 0);
    expect(d.back() == 29);

    // Phase B — 20 ops: push_front 100..119.  Deque: 119 118 .. 100 0 1 .. 29.
    i = 0;
    while (i < 20) : (i += 1) {
        d.push_front(a, 100 + i);
    }
    expect(d.len() == 50);
    expect(d.front() == 119);
    expect(d.back() == 29);

    // Phase C — 25 ops: pop_front yields 119 down to 100, then 0 1 2 3 4.
    // Sum = (100+...+119) + (0+1+2+3+4) = 2190 + 10 = 2200.
    var sum_c: i32 = 0;
    var c: i32 = 0;
    while (c < 25) : (c += 1) {
        var pc: i32 = d.pop_front();
        if (c == 0) {
            expect(pc == 119);
        }
        sum_c += pc;
    }
    expect(sum_c == 2200);
    expect(d.len() == 25);
    expect(d.front() == 5);
    expect(d.back() == 29);

    // Phase D — 25 ops: pop_back yields 29 down to 5.
    // Sum = 5+6+...+29 = 425.
    var sum_d: i32 = 0;
    var k: i32 = 0;
    while (k < 25) : (k += 1) {
        var pd: i32 = d.pop_back();
        if (k == 0) {
            expect(pd == 29);
        }
        sum_d += pd;
    }
    expect(sum_d == 425);
    expect(d.len() == 0);
    expect(d.is_empty());
    d.deinit(a);
}

test "deque: property — multiset sum preserved through mixed ends" {
    var a: Allocator = c_allocator();
    var d: Deque(i32) = Deque(i32).init(a);
    // 37 pseudo-random values in [-50, 50] (includes negatives, min -47,
    // max 49), alternating push_front / push_back.
    var sum_in: i32 = 0;
    var i: i32 = 0;
    while (i < 37) : (i += 1) {
        var v: i32 = (i * i * 31 + 7) % 101 - 50;
        sum_in += v;
        if (i % 2 == 0) {
            d.push_front(a, v);
        } else {
            d.push_back(a, v);
        }
    }
    expect(sum_in == 138);   // hand-pinned: sum of the 37 generated values
    expect(d.len() == 37);
    // Drain alternating pop_back / pop_front: same multiset comes out.
    var sum_out: i32 = 0;
    var pops: i32 = 0;
    while (!d.is_empty()) {
        if (pops % 2 == 0) {
            sum_out += d.pop_back();
        } else {
            sum_out += d.pop_front();
        }
        pops += 1;
    }
    expect(pops == 37);
    expect(sum_out == 138);
    expect(sum_out == sum_in);
    expect(d.len() == 0);
    d.deinit(a);
}

test "deque: i64 extremes survive a round trip" {
    var a: Allocator = c_allocator();
    var d: Deque(i64) = Deque(i64).init(a);
    var big: i64 = 9223372036854775807;
    var small: i64 = 0 - big - 1;    // i64 min
    d.push_back(a, big);
    d.push_front(a, small);
    d.push_back(a, 0);
    expect(d.len() == 3);
    expect(d.front() == small);
    expect(d.back() == 0);
    expect(d.pop_back() == 0);
    expect(d.pop_back() == big);
    expect(d.pop_front() == small);
    expect(d.is_empty());
    d.deinit(a);
}
