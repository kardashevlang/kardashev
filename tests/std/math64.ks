// tests/std/math64.ks — std math64 module tests (v0.154 std WAVE 1).
//
// Every public fn is exercised: normal cases, boundaries (zero, negatives,
// i64 extremes) and property-style sweeps. All expected values are
// hand-computed. Run with `kard test tests/std/math64.ks`.

@import("std");

test "imin64/imax64" {
    expect(imin64(2, 3) == 2);
    expect(imin64(3, 2) == 2);
    expect(imax64(2, 3) == 3);
    expect(imax64(3, 2) == 3);
    expect(imin64(5, 5) == 5);
    expect(imax64(5, 5) == 5);
    expect(imin64(0 - 4, 1) == 0 - 4);
    expect(imax64(0 - 4, 1) == 1);
    expect(imin64(0 - 9, 0 - 2) == 0 - 9);
    expect(imax64(0 - 9, 0 - 2) == 0 - 2);
    // i64 extremes (min is built as (-(2^63-1)) - 1; no literal for it).
    var mx: i64 = 9223372036854775807;
    var mn: i64 = (0 - 9223372036854775807) - 1;
    expect(imin64(mn, mx) == mn);
    expect(imax64(mn, mx) == mx);
    expect(imin64(mx, mx) == mx);
    expect(imax64(mn, mn) == mn);
}

test "iabs64 and sign" {
    expect(iabs64(0) == 0);
    expect(iabs64(7) == 7);
    expect(iabs64(0 - 7) == 7);
    var mx: i64 = 9223372036854775807;
    expect(iabs64(mx) == mx);
    expect(iabs64(0 - mx) == mx);
    expect(sign(0) == 0);
    expect(sign(42) == 1);
    expect(sign(0 - 42) == 0 - 1);
    expect(sign(mx) == 1);
    var mn: i64 = (0 - 9223372036854775807) - 1;
    expect(sign(mn) == 0 - 1);
}

test "clamp64" {
    expect(clamp64(5, 0, 10) == 5);     // inside
    expect(clamp64(0 - 3, 0, 10) == 0); // below
    expect(clamp64(99, 0, 10) == 10);   // above
    expect(clamp64(0, 0, 10) == 0);     // at lo
    expect(clamp64(10, 0, 10) == 10);   // at hi
    expect(clamp64(123, 7, 7) == 7);    // degenerate lo == hi
    expect(clamp64(0 - 8, 0 - 5, 0 - 1) == 0 - 5);  // all-negative range
    expect(clamp64(0 - 3, 0 - 5, 0 - 1) == 0 - 3);
    var mx: i64 = 9223372036854775807;
    var mn: i64 = (0 - 9223372036854775807) - 1;
    expect(clamp64(mx, 0 - 5, 5) == 5);
    expect(clamp64(mn, 0 - 5, 5) == 0 - 5);
    expect(clamp64(3, mn, mx) == 3);
}

test "gcd table" {
    expect(gcd(0, 0) == 0);
    expect(gcd(0, 5) == 5);
    expect(gcd(5, 0) == 5);
    expect(gcd(12, 18) == 6);
    expect(gcd(0 - 12, 18) == 6);       // non-negative on every sign combo
    expect(gcd(12, 0 - 18) == 6);
    expect(gcd(0 - 12, 0 - 18) == 6);
    expect(gcd(17, 13) == 1);           // coprime
    expect(gcd(1071, 462) == 21);       // the classic Euclid example
    expect(gcd(48, 36) == 12);
    expect(gcd(4611686018427387904, 2147483648) == 2147483648);  // 2^62, 2^31
    var mx: i64 = 9223372036854775807;
    expect(gcd(mx, mx) == mx);
    expect(gcd(mx, 1) == 1);
}

test "lcm table" {
    expect(lcm(0, 0) == 0);
    expect(lcm(0, 7) == 0);
    expect(lcm(7, 0) == 0);
    expect(lcm(4, 6) == 12);
    expect(lcm(0 - 4, 6) == 12);        // non-negative on every sign combo
    expect(lcm(4, 0 - 6) == 12);
    expect(lcm(0 - 4, 0 - 6) == 12);
    expect(lcm(7, 13) == 91);           // coprime -> product
    expect(lcm(21, 6) == 42);
    expect(lcm(1, 1) == 1);
    expect(lcm(2, 4611686018427387904) == 4611686018427387904);  // 2^62 in range
}

test "ipow boundaries" {
    expect(ipow(0, 0) == 1);            // documented convention
    expect(ipow(5, 0) == 1);
    expect(ipow(0, 5) == 0);
    expect(ipow(1, 100) == 1);
    expect(ipow(2, 10) == 1024);
    expect(ipow(3, 4) == 81);
    expect(ipow(10, 18) == 1000000000000000000);
    expect(ipow(2, 62) == 4611686018427387904);  // largest in-range power of 2
    expect(ipow(0 - 2, 2) == 4);
    expect(ipow(0 - 2, 3) == 0 - 8);
    expect(ipow(0 - 1, 101) == 0 - 1);
    expect(ipow(0 - 3, 3) == 0 - 27);
    expect(ipow(7, 0 - 1) == 0);        // negative exponent -> 0
    expect(ipow(0, 0 - 3) == 0);
}

test "isqrt exactness" {
    expect(isqrt(0 - 9) == 0);          // negative -> 0
    expect(isqrt(0) == 0);
    expect(isqrt(1) == 1);
    expect(isqrt(2) == 1);
    expect(isqrt(3) == 1);
    expect(isqrt(4) == 2);
    expect(isqrt(8) == 2);
    expect(isqrt(9) == 3);
    expect(isqrt(15) == 3);
    expect(isqrt(16) == 4);
    expect(isqrt(24) == 4);
    expect(isqrt(25) == 5);
    expect(isqrt(999999999999999999) == 999999999);    // 10^18 - 1
    expect(isqrt(1000000000000000000) == 1000000000);  // 10^18 = (10^9)^2
    // n = 3037000499 is the largest n with n*n <= i64 max:
    // n*n = 9223372030926249001.
    expect(isqrt(9223372030926249001) == 3037000499);  // exact square
    expect(isqrt(9223372030926249000) == 3037000498);  // n*n - 1
    expect(isqrt(9223372036854775807) == 3037000499);  // i64 max
}

test "div_floor/mod_floor all sign combos" {
    // C `/`/`%` truncate toward zero — pin the contrast first.
    expect((0 - 7) / 2 == 0 - 3);
    expect((0 - 7) % 2 == 0 - 1);
    // floor semantics: quotient rounds toward -inf, remainder has b's sign.
    expect(div_floor(7, 2) == 3);
    expect(mod_floor(7, 2) == 1);
    expect(div_floor(0 - 7, 2) == 0 - 4);
    expect(mod_floor(0 - 7, 2) == 1);
    expect(div_floor(7, 0 - 2) == 0 - 4);
    expect(mod_floor(7, 0 - 2) == 0 - 1);
    expect(div_floor(0 - 7, 0 - 2) == 3);
    expect(mod_floor(0 - 7, 0 - 2) == 0 - 1);
    // exact divisions: remainder 0, quotient == truncating quotient.
    expect(div_floor(6, 3) == 2);
    expect(mod_floor(6, 3) == 0);
    expect(div_floor(0 - 6, 3) == 0 - 2);
    expect(mod_floor(0 - 6, 3) == 0);
    expect(div_floor(6, 0 - 3) == 0 - 2);
    expect(mod_floor(6, 0 - 3) == 0);
    expect(div_floor(0 - 6, 0 - 3) == 2);
    expect(mod_floor(0 - 6, 0 - 3) == 0);
    // a == 0
    expect(div_floor(0, 5) == 0);
    expect(mod_floor(0, 5) == 0);
    expect(div_floor(0, 0 - 5) == 0);
    expect(mod_floor(0, 0 - 5) == 0);
}

test "property: isqrt floor invariant on 0..3000" {
    var x: i64 = 0;
    while (x <= 3000) : (x += 1) {
        var r: i64 = isqrt(x);
        expect(r * r <= x);
        expect((r + 1) * (r + 1) > x);
    }
}

test "property: div_floor/mod_floor reconstruct a" {
    var bs: [6]i64 = [6]i64{ 0 - 7, 0 - 3, 0 - 2, 2, 3, 7 };
    var a: i64 = 0 - 24;
    while (a <= 24) : (a += 1) {
        var j: usize = 0;
        while (j < bs.len) : (j += 1) {
            var b: i64 = bs[j];
            var q: i64 = div_floor(a, b);
            var m: i64 = mod_floor(a, b);
            expect(b * q + m == a);
            if (b > 0) {
                expect(m >= 0 and m < b);     // remainder takes b's sign
            } else {
                expect(m <= 0 and m > b);
            }
        }
    }
}

test "property: gcd divides both, gcd*lcm == a*b" {
    var a: i64 = 1;
    while (a <= 30) : (a += 1) {
        var b: i64 = 1;
        while (b <= 30) : (b += 1) {
            var g: i64 = gcd(a, b);
            expect(g >= 1);
            expect(a % g == 0);
            expect(b % g == 0);
            expect(g * lcm(a, b) == a * b);
        }
    }
}

test "property: ipow matches naive multiply" {
    var base: i64 = 0 - 3;
    while (base <= 3) : (base += 1) {
        var e: i64 = 0;
        while (e <= 10) : (e += 1) {
            var naive: i64 = 1;
            var k: i64 = 0;
            while (k < e) : (k += 1) {
                naive = naive * base;
            }
            expect(ipow(base, e) == naive);
        }
    }
}

test "property: clamp64 == imin64(imax64(x, lo), hi)" {
    var x: i64 = 0 - 15;
    while (x <= 15) : (x += 1) {
        expect(clamp64(x, 0 - 6, 9) == imin64(imax64(x, 0 - 6), 9));
        expect(sign(clamp64(x, 0 - 6, 9)) == sign(imin64(imax64(x, 0 - 6), 9)));
    }
}
