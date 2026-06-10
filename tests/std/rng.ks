// tests/std/rng.ks — std `rng` module tests (v0.154 std WAVE 1).
//
// All expected values are hand-computed (replicated in Python) from the
// xorshift64* recurrence: x ^= x>>12; x ^= x<<25; x ^= x>>27;
// output = state * 2685821657736338717 mod 2^64.

@import("std");

test "rng: xorshift64* pinned outputs for seed 1" {
    var r: Rng = Rng.init(1);
    // v1 = 5180492295206395165 (fits i64, pin whole + split)
    var v1: u64 = r.next_u64();
    expect(v1 == 5180492295206395165);
    expect((v1 >> 32) == 1206177355);
    expect((v1 % 4294967296) == 2305613085);
    // v2 = 12380297144915551517 (> i64 max: pin via hi/lo 32-bit halves)
    var v2: u64 = r.next_u64();
    expect((v2 >> 32) == 2882512552);
    expect((v2 % 4294967296) == 3766052125);
    // v3 = 13389498078930870103
    var v3: u64 = r.next_u64();
    expect((v3 >> 32) == 3117485455);
    expect((v3 % 4294967296) == 3950190423);
    // v4 = 5599127315341312413
    var v4: u64 = r.next_u64();
    expect(v4 == 5599127315341312413);
    expect((v4 >> 32) == 1303648416);
    expect((v4 % 4294967296) == 3139109277);
}

test "rng: seed 0 maps to the documented nonzero constant" {
    var r0: Rng = Rng.init(0);
    var rs: Rng = Rng.init(88172645463325252);
    var a: u64 = r0.next_u64();
    var b: u64 = rs.next_u64();
    expect(a == b);
    // first output for that state = 16620430977058721579
    expect((a >> 32) == 3869745642);
    expect((a % 4294967296) == 830197547);
}

test "rng: same seed yields the same sequence" {
    var r1: Rng = Rng.init(42);
    var r2: Rng = Rng.init(42);
    var i: i32 = 0;
    var same: bool = true;
    while (i < 50) : (i += 1) {
        if (r1.next_u64() != r2.next_u64()) {
            same = false;
        }
    }
    expect(same);
    // first seed-42 output, pinned: 6255019084209693600
    var r3: Rng = Rng.init(42);
    expect(r3.next_u64() == 6255019084209693600);
    // a different seed diverges immediately
    var r4: Rng = Rng.init(43);
    expect(r4.next_u64() != 6255019084209693600);
}

test "rng: next_below stays below n over 1000 draws" {
    var r: Rng = Rng.init(99);
    var i: i32 = 0;
    var ok: bool = true;
    while (i < 1000) : (i += 1) {
        if (r.next_below(7) >= 7) {
            ok = false;
        }
    }
    expect(ok);
}

test "rng: next_below pinned values and edge cases" {
    // seed 5, n = 7: first 6 draws are 1 2 6 0 6 4
    var r: Rng = Rng.init(5);
    expect(r.next_below(7) == 1);
    expect(r.next_below(7) == 2);
    expect(r.next_below(7) == 6);
    expect(r.next_below(7) == 0);
    expect(r.next_below(7) == 6);
    expect(r.next_below(7) == 4);
    // n == 0 -> 0 (documented), n == 1 -> always 0
    expect(r.next_below(0) == 0);
    expect(r.next_below(1) == 0);
    expect(r.next_below(1) == 0);
}

test "rng: next_i64_in pinned values for seed 7 over [-5, 5]" {
    var r: Rng = Rng.init(7);
    expect(r.next_i64_in(0 - 5, 5) == 0 - 4);
    expect(r.next_i64_in(0 - 5, 5) == 4);
    expect(r.next_i64_in(0 - 5, 5) == 0 - 2);
    expect(r.next_i64_in(0 - 5, 5) == 1);
    expect(r.next_i64_in(0 - 5, 5) == 0 - 3);
}

test "rng: next_i64_in bounds, negatives, degenerate ranges" {
    // 1000 draws over [-5, 5] all stay inclusive-in-range
    var r: Rng = Rng.init(11);
    var i: i32 = 0;
    var ok: bool = true;
    while (i < 1000) : (i += 1) {
        var v: i64 = r.next_i64_in(0 - 5, 5);
        if (v < 0 - 5) {
            ok = false;
        }
        if (v > 5) {
            ok = false;
        }
    }
    expect(ok);
    // all-negative range, pinned first 5 for seed 8: -8 -6 -9 -10 -9
    var rn: Rng = Rng.init(8);
    expect(rn.next_i64_in(0 - 10, 0 - 1) == 0 - 8);
    expect(rn.next_i64_in(0 - 10, 0 - 1) == 0 - 6);
    expect(rn.next_i64_in(0 - 10, 0 - 1) == 0 - 9);
    expect(rn.next_i64_in(0 - 10, 0 - 1) == 0 - 10);
    expect(rn.next_i64_in(0 - 10, 0 - 1) == 0 - 9);
    // lo == hi returns lo and consumes no draw
    var rd: Rng = Rng.init(5);
    expect(rd.next_i64_in(3, 3) == 3);
    expect(rd.next_below(7) == 1); // still the first seed-5 draw
    // lo > hi (degenerate) returns lo
    expect(rd.next_i64_in(5, 0 - 5) == 5);
}

test "rng: next_i64_in over the full i64 range equals a raw signed draw" {
    var lo: i64 = 0 - 9223372036854775807 - 1;
    var hi: i64 = 9223372036854775807;
    var r1: Rng = Rng.init(3);
    var v: i64 = r1.next_i64_in(lo, hi);
    var r2: Rng = Rng.init(3);
    var w: i64 = @as(i64, r2.next_u64());
    expect(v == w);
    // pinned: seed-3 first raw output as i64 is -2905267188090366121
    expect(v == 0 - 2905267188090366121);
}

test "rng: shuffle pins the seed-1 permutation of 0..31" {
    var a: Allocator = c_allocator();
    var xs: []i64 = alloc(a, i64, 32);
    var i: usize = 0;
    while (i < 32) : (i += 1) {
        xs[i] = @as(i64, i);
    }
    var r: Rng = Rng.init(1);
    shuffle(i64, &r, xs);
    // full result: 1 31 22 21 24 9 17 14 10 25 6 3 27 19 15 2
    //              20 7 5 26 16 18 0 8 11 23 4 12 30 13 28 29
    expect(xs[0] == 1);
    expect(xs[1] == 31);
    expect(xs[2] == 22);
    expect(xs[3] == 21);
    expect(xs[4] == 24);
    // the order really changed from 0 1 2 3 4 ...
    expect(xs[0] != 0);
    free(a, xs);
}

test "rng: shuffle preserves the multiset (sum + insertion-sorted compare)" {
    var a: Allocator = c_allocator();
    var xs: []i64 = alloc(a, i64, 32);
    var i: usize = 0;
    while (i < 32) : (i += 1) {
        xs[i] = @as(i64, i);
    }
    var r: Rng = Rng.init(1);
    shuffle(i64, &r, xs);
    // property 1: the sum is unchanged (0+1+...+31 = 496)
    var sum: i64 = 0;
    for (xs) |x| {
        sum += x;
    }
    expect(sum == 496);
    // property 2: insertion-sorting the shuffled slice yields exactly 0..31
    // (local sort so this suite depends on no other std module)
    var s: usize = 1;
    while (s < 32) : (s += 1) {
        var key: i64 = xs[s];
        var t: usize = s;
        while (t > 0) {
            if (xs[t - 1] <= key) {
                break;
            }
            xs[t] = xs[t - 1];
            t -= 1;
        }
        xs[t] = key;
    }
    var m: usize = 0;
    var sorted_ok: bool = true;
    while (m < 32) : (m += 1) {
        if (xs[m] != @as(i64, m)) {
            sorted_ok = false;
        }
    }
    expect(sorted_ok);
    free(a, xs);
}

test "rng: shuffle on empty and single-element slices consumes no draws" {
    var a: Allocator = c_allocator();
    var r: Rng = Rng.init(1);
    var e: []i64 = alloc(a, i64, 0);
    shuffle(i64, &r, e);
    expect(e.len == 0);
    var one: []i64 = alloc(a, i64, 1);
    one[0] = 7;
    shuffle(i64, &r, one);
    expect(one[0] == 7);
    // neither call drew from r: the next output is still the seed-1 first one
    expect(r.next_u64() == 5180492295206395165);
    free(a, one);
    free(a, e);
}

test "rng: shuffle is generic and deterministic across element types" {
    var a: Allocator = c_allocator();
    // i32 elements, seed 2: [10, 20, 30, 40] -> [40, 20, 10, 30]
    var zs: []i32 = alloc(a, i32, 4);
    zs[0] = 10;
    zs[1] = 20;
    zs[2] = 30;
    zs[3] = 40;
    var r: Rng = Rng.init(2);
    shuffle(i32, &r, zs);
    expect(zs[0] == 40);
    expect(zs[1] == 20);
    expect(zs[2] == 10);
    expect(zs[3] == 30);
    // same seed -> identical permutation on a second run
    var ws: []i32 = alloc(a, i32, 4);
    ws[0] = 10;
    ws[1] = 20;
    ws[2] = 30;
    ws[3] = 40;
    var r2: Rng = Rng.init(2);
    shuffle(i32, &r2, ws);
    var k: usize = 0;
    var same: bool = true;
    while (k < 4) : (k += 1) {
        if (ws[k] != zs[k]) {
            same = false;
        }
    }
    expect(same);
    free(a, ws);
    free(a, zs);
}
