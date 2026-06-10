// std.ks — the kardashev standard library, bundled into the compiler and
// pulled in with `@import("std");` (v0.145).
//
// It is flattened into the importing program (#include-style), so after the
// import its public items are available by bare name: `ArrayList(i32)`,
// `HashMap(i32)`, `imin`, `imax`, etc. Written entirely in the language.

// --- small numeric helpers -------------------------------------------------

pub fn imin(a: i32, b: i32) i32 {
    if (a < b) {
        return a;
    }
    return b;
}

pub fn imax(a: i32, b: i32) i32 {
    if (a > b) {
        return a;
    }
    return b;
}

pub fn iabs(x: i32) i32 {
    if (x < 0) {
        return 0 - x;
    }
    return x;
}

// --- string utilities (over `[]u8`) ----------------------------------------

// Byte-for-byte equality of two strings.
pub fn str_eq(a: []u8, b: []u8) bool {
    if (a.len != b.len) {
        return false;
    }
    var i: usize = 0;
    while (i < a.len) : (i += 1) {
        if (a[i] != b[i]) {
            return false;
        }
    }
    return true;
}

// Does `s` begin with `prefix`?
pub fn str_starts_with(s: []u8, prefix: []u8) bool {
    if (prefix.len > s.len) {
        return false;
    }
    var i: usize = 0;
    while (i < prefix.len) : (i += 1) {
        if (s[i] != prefix[i]) {
            return false;
        }
    }
    return true;
}

// Index of the first byte equal to `c`, or -1 if absent.
pub fn str_index_of(s: []u8, c: u8) i32 {
    var i: usize = 0;
    while (i < s.len) : (i += 1) {
        if (s[i] == c) {
            return @as(i32, i);
        }
    }
    return 0 - 1;
}

// Concatenate `x` and `y` into a freshly-allocated `[]u8` (free it with
// `free(a, result)`).
pub fn str_concat(a: Allocator, x: []u8, y: []u8) []u8 {
    var out: []u8 = alloc(a, u8, x.len + y.len);
    var i: usize = 0;
    while (i < x.len) : (i += 1) {
        out[i] = x[i];
    }
    var j: usize = 0;
    while (j < y.len) : (j += 1) {
        out[x.len + j] = y[j];
    }
    return out;
}

// --- ArrayList(V) — a growable list -----------------------------------------

pub fn ArrayList(comptime V: type) type {
    return struct {
        items: []V,
        count: usize,

        fn init(a: Allocator) Self {
            return Self{ .items = alloc(a, V, 4), .count = 0 };
        }

        fn push(self: *Self, a: Allocator, x: V) void {
            if (self.count == self.items.len) {
                var grown: []V = alloc(a, V, self.items.len * 2);
                var i: usize = 0;
                while (i < self.count) : (i += 1) {
                    grown[i] = self.items[i];
                }
                free(a, self.items);
                self.items = grown;
            }
            self.items[self.count] = x;
            self.count += 1;
        }

        fn get(self: Self, i: usize) V {
            return self.items[i];
        }

        fn set(self: *Self, i: usize, x: V) void {
            self.items[i] = x;
        }

        fn len(self: Self) usize {
            return self.count;
        }

        fn deinit(self: Self, a: Allocator) void {
            free(a, self.items);
        }
    };
}

// --- HashMap(V) — an i32-keyed open-addressing map --------------------------

const SLOT_EMPTY: i32 = 0;
const SLOT_FULL: i32 = 1;
const SLOT_TOMB: i32 = 2;

pub fn HashMap(comptime V: type) type {
    return struct {
        keys: []i32,
        vals: []V,
        state: []i32,
        cap: usize,
        count: usize,

        fn with_cap(a: Allocator, c: usize) Self {
            var s: Self = Self{
                .keys = alloc(a, i32, c),
                .vals = alloc(a, V, c),
                .state = alloc(a, i32, c),
                .cap = c,
                .count = 0,
            };
            var i: usize = 0;
            while (i < c) : (i += 1) {
                s.state[i] = SLOT_EMPTY;
            }
            return s;
        }

        fn init(a: Allocator) Self {
            return Self.with_cap(a, 8);
        }

        fn find(self: Self, key: i32) usize {
            var i: usize = @as(usize, iabs(key)) % self.cap;
            while (self.state[i] != SLOT_EMPTY) : (i = (i + 1) % self.cap) {
                if (self.state[i] == SLOT_FULL) {
                    if (self.keys[i] == key) {
                        return i;
                    }
                }
            }
            return i;
        }

        fn grow(self: *Self, a: Allocator) void {
            var ok: []i32 = self.keys;
            var ov: []V = self.vals;
            var os: []i32 = self.state;
            var oc: usize = self.cap;
            var fresh: Self = Self.with_cap(a, oc * 2);
            var i: usize = 0;
            while (i < oc) : (i += 1) {
                if (os[i] == SLOT_FULL) {
                    fresh.insert(ok[i], ov[i]);
                }
            }
            free(a, ok);
            free(a, ov);
            free(a, os);
            self.keys = fresh.keys;
            self.vals = fresh.vals;
            self.state = fresh.state;
            self.cap = fresh.cap;
            self.count = fresh.count;
        }

        fn insert(self: *Self, key: i32, val: V) void {
            var i: usize = self.find(key);
            if (self.state[i] != SLOT_FULL) {
                self.state[i] = SLOT_FULL;
                self.keys[i] = key;
                self.count += 1;
            }
            self.vals[i] = val;
        }

        fn put(self: *Self, a: Allocator, key: i32, val: V) void {
            if ((self.count + 1) * 4 >= self.cap * 3) {
                self.grow(a);
            }
            self.insert(key, val);
        }

        fn has(self: Self, key: i32) bool {
            return self.state[self.find(key)] == SLOT_FULL;
        }

        fn get(self: Self, key: i32, fallback: V) V {
            var i: usize = self.find(key);
            if (self.state[i] == SLOT_FULL) {
                return self.vals[i];
            }
            return fallback;
        }

        fn remove(self: *Self, key: i32) bool {
            var i: usize = self.find(key);
            if (self.state[i] == SLOT_FULL) {
                self.state[i] = SLOT_TOMB;
                self.count -= 1;
                return true;
            }
            return false;
        }

        fn len(self: Self) usize {
            return self.count;
        }

        fn deinit(self: Self, a: Allocator) void {
            free(a, self.keys);
            free(a, self.vals);
            free(a, self.state);
        }
    };
}

// --- math64 — 64-bit integer math ------------------------------------------
//
// i64 counterparts of the i32 helpers above, plus gcd/lcm, fast integer
// exponentiation, floor square root and floor-division/remainder. All
// functions are written to avoid C signed-overflow UB on in-range inputs;
// the per-function caveats below mark the inputs that cannot be represented
// (e.g. `iabs64` of the minimum i64).

/// Smaller of two `i64` values.
pub fn imin64(a: i64, b: i64) i64 {
    if (a < b) {
        return a;
    }
    return b;
}

/// Larger of two `i64` values.
pub fn imax64(a: i64, b: i64) i64 {
    if (a > b) {
        return a;
    }
    return b;
}

/// Absolute value of an `i64`. Caveat: the minimum i64
/// (-9223372036854775808) has no positive counterpart, so `x` must be
/// greater than it.
pub fn iabs64(x: i64) i64 {
    if (x < 0) {
        return 0 - x;
    }
    return x;
}

/// Sign of `x`: -1 if negative, 0 if zero, 1 if positive.
pub fn sign(x: i64) i64 {
    if (x < 0) {
        return 0 - 1;
    }
    if (x > 0) {
        return 1;
    }
    return 0;
}

/// Clamp `x` into the inclusive range `[lo, hi]` (assumes `lo <= hi`).
pub fn clamp64(x: i64, lo: i64, hi: i64) i64 {
    if (x < lo) {
        return lo;
    }
    if (x > hi) {
        return hi;
    }
    return x;
}

/// Greatest common divisor of `a` and `b` (Euclid). The result is
/// non-negative and `gcd(0, 0) == 0`. Caveat: takes absolute values first,
/// so neither argument may be the minimum i64.
pub fn gcd(a: i64, b: i64) i64 {
    var x: i64 = iabs64(a);
    var y: i64 = iabs64(b);
    while (y != 0) {
        var t: i64 = x % y;
        x = y;
        y = t;
    }
    return x;
}

/// Least common multiple of `a` and `b`: 0 if either is 0, otherwise the
/// non-negative lcm. Caveat: overflows i64 (C UB) when the true lcm exceeds
/// 9223372036854775807 — the caller must keep |a|/gcd * |b| in range.
pub fn lcm(a: i64, b: i64) i64 {
    if (a == 0 or b == 0) {
        return 0;
    }
    var g: i64 = gcd(a, b);
    return (iabs64(a) / g) * iabs64(b);
}

/// `base` raised to `exp` by squaring (O(log exp)). `exp < 0` returns 0;
/// `ipow(0, 0) == 1` (the usual integer-math convention). Caveat: the
/// mathematical result must fit in i64 or the multiplications overflow
/// (C UB); in-range results like `ipow(2, 62)` are computed exactly —
/// the squared base is never computed past the final step.
pub fn ipow(base: i64, exp: i64) i64 {
    if (exp < 0) {
        return 0;
    }
    var result: i64 = 1;
    var b: i64 = base;
    var e: i64 = exp;
    while (e > 0) : (e = e / 2) {
        if (e % 2 == 1) {
            result = result * b;
        }
        if (e > 1) {
            b = b * b;
        }
    }
    return result;
}

/// Floor of the square root of `x`; negative `x` returns 0. Integer Newton
/// iteration from an above-the-root initial guess — exact for all i64,
/// including 9223372036854775807 -> 3037000499, and never forms `(r+1)^2`,
/// so there is no overflow near the top of the range.
pub fn isqrt(x: i64) i64 {
    if (x < 0) {
        return 0;
    }
    if (x < 2) {
        return x;
    }
    var r: i64 = x / 2 + 1;
    var nr: i64 = (r + x / r) / 2;
    while (nr < r) {
        r = nr;
        nr = (r + x / r) / 2;
    }
    return r;
}

/// Floor division (rounds toward negative infinity, unlike `/` which
/// truncates toward zero): `div_floor(-7, 2) == -4`. `b` must be non-zero
/// (division by zero is C UB).
pub fn div_floor(a: i64, b: i64) i64 {
    var q: i64 = a / b;
    if (a % b != 0 and ((a < 0) != (b < 0))) {
        return q - 1;
    }
    return q;
}

/// Euclidean-style floor remainder, the companion of `div_floor`:
/// `a == b * div_floor(a, b) + mod_floor(a, b)`, and the result has the
/// sign of `b` (or is 0) — `mod_floor(-7, 2) == 1`. `b` must be non-zero
/// (division by zero is C UB).
pub fn mod_floor(a: i64, b: i64) i64 {
    var r: i64 = a % b;
    if (r != 0 and ((r < 0) != (b < 0))) {
        return r + b;
    }
    return r;
}

// --- slices — generic slice algorithms (v0.154) -----------------------------
//
// Free generic functions over `[]T` (call style: `sort(i64, xs)`). All
// in-place operations require the slice's backing storage to be mutable.

// Insertion sort of xs[lo..=hi] (inclusive i64 bounds). Used by sort() for
// short runs; stable shifting of elements greater than the inserted value.
fn slices_insertion(comptime T: type, xs: []T, lo: i64, hi: i64) void {
    var i: i64 = lo + 1;
    while (i <= hi) : (i += 1) {
        var x: T = xs[@as(usize, i)];
        var j: i64 = i - 1;
        while (j >= lo) {
            if (xs[@as(usize, j)] > x) {
                xs[@as(usize, j + 1)] = xs[@as(usize, j)];
                j -= 1;
            } else {
                break;
            }
        }
        xs[@as(usize, j + 1)] = x;
    }
}

// Recursive quicksort of xs[lo..=hi]: median-of-3 pivot, Hoare-style
// partition, insertion sort for sub-ranges shorter than 17 elements. Both
// partitions are strictly smaller than the input range, so it terminates
// (including on all-equal inputs).
fn slices_qsort(comptime T: type, xs: []T, lo: i64, hi: i64) void {
    if (hi - lo < 16) {
        if (lo < hi) {
            slices_insertion(T, xs, lo, hi);
        }
        return;
    }
    var mid: i64 = lo + (hi - lo) / 2;
    // Median-of-3: order xs[lo] <= xs[mid] <= xs[hi]; the median lands at mid.
    if (xs[@as(usize, mid)] < xs[@as(usize, lo)]) {
        var t0: T = xs[@as(usize, lo)];
        xs[@as(usize, lo)] = xs[@as(usize, mid)];
        xs[@as(usize, mid)] = t0;
    }
    if (xs[@as(usize, hi)] < xs[@as(usize, lo)]) {
        var t1: T = xs[@as(usize, lo)];
        xs[@as(usize, lo)] = xs[@as(usize, hi)];
        xs[@as(usize, hi)] = t1;
    }
    if (xs[@as(usize, hi)] < xs[@as(usize, mid)]) {
        var t2: T = xs[@as(usize, mid)];
        xs[@as(usize, mid)] = xs[@as(usize, hi)];
        xs[@as(usize, hi)] = t2;
    }
    var pivot: T = xs[@as(usize, mid)];
    var i: i64 = lo;
    var j: i64 = hi;
    while (i <= j) {
        while (xs[@as(usize, i)] < pivot) {
            i += 1;
        }
        while (xs[@as(usize, j)] > pivot) {
            j -= 1;
        }
        if (i <= j) {
            var t: T = xs[@as(usize, i)];
            xs[@as(usize, i)] = xs[@as(usize, j)];
            xs[@as(usize, j)] = t;
            i += 1;
            j -= 1;
        }
    }
    slices_qsort(T, xs, lo, j);
    slices_qsort(T, xs, i, hi);
}

/// Sort `xs` in place, ascending. Quicksort (median-of-3 pivot) switching to
/// insertion sort for runs shorter than 17 elements; not stable.
pub fn sort(comptime T: type, xs: []T) void {
    if (xs.len < 2) {
        return;
    }
    slices_qsort(T, xs, 0, @as(i64, xs.len) - 1);
}

/// Reverse `xs` in place.
pub fn reverse(comptime T: type, xs: []T) void {
    if (xs.len < 2) {
        return;
    }
    var i: usize = 0;
    var j: usize = xs.len - 1;
    while (i < j) {
        var t: T = xs[i];
        xs[i] = xs[j];
        xs[j] = t;
        i += 1;
        j -= 1;
    }
}

/// Binary search in ascending-sorted `xs`: an index holding `x`, or -1 if
/// absent. With duplicates, any matching index may be returned.
pub fn binary_search(comptime T: type, xs: []T, x: T) i64 {
    var lo: i64 = 0;
    var hi: i64 = @as(i64, xs.len) - 1;
    while (lo <= hi) {
        var mid: i64 = lo + (hi - lo) / 2;
        var v: T = xs[@as(usize, mid)];
        if (v == x) {
            return mid;
        }
        if (v < x) {
            lo = mid + 1;
        } else {
            hi = mid - 1;
        }
    }
    return 0 - 1;
}

/// First index of `x` in `xs` (linear scan), or -1 if absent.
pub fn index_of_elem(comptime T: type, xs: []T, x: T) i64 {
    var i: usize = 0;
    while (i < xs.len) : (i += 1) {
        if (xs[i] == x) {
            return @as(i64, i);
        }
    }
    return 0 - 1;
}

/// Does `xs` contain `x`?
pub fn contains(comptime T: type, xs: []T, x: T) bool {
    return index_of_elem(T, xs, x) >= 0;
}

/// Set every element of `xs` to `v`.
pub fn fill(comptime T: type, xs: []T, v: T) void {
    var i: usize = 0;
    while (i < xs.len) : (i += 1) {
        xs[i] = v;
    }
}

/// Copy the first `min(dst.len, src.len)` elements of `src` into `dst`.
/// The extra tail of the longer slice is left untouched.
pub fn copy_into(comptime T: type, dst: []T, src: []T) void {
    var n: usize = src.len;
    if (dst.len < n) {
        n = dst.len;
    }
    var i: usize = 0;
    while (i < n) : (i += 1) {
        dst[i] = src[i];
    }
}

/// Sum of all elements (empty slice -> 0). The caller must ensure the total
/// fits in i64; overflow is not checked.
pub fn sum64(xs: []i64) i64 {
    var acc: i64 = 0;
    for (xs) |x| {
        acc += x;
    }
    return acc;
}

/// Smallest element of `xs` (empty slice -> i64 max, 9223372036854775807).
pub fn min_in(xs: []i64) i64 {
    var best: i64 = 9223372036854775807;
    for (xs) |x| {
        if (x < best) {
            best = x;
        }
    }
    return best;
}

/// Largest element of `xs` (empty slice -> i64 min, -9223372036854775808).
pub fn max_in(xs: []i64) i64 {
    var best: i64 = (0 - 9223372036854775807) - 1;
    for (xs) |x| {
        if (x > best) {
            best = x;
        }
    }
    return best;
}

/// Is `xs` sorted ascending (non-decreasing)? Empty and single-element
/// slices are sorted.
pub fn is_sorted(comptime T: type, xs: []T) bool {
    if (xs.len < 2) {
        return true;
    }
    var i: usize = 1;
    while (i < xs.len) : (i += 1) {
        if (xs[i - 1] > xs[i]) {
            return false;
        }
    }
    return true;
}


// --- text: integer parse/format + StrBuilder ---------------------------------

/// Parse a decimal integer (optional leading '-', digits only). Returns null
/// on an empty string, a stray sign, any non-digit byte, or i64 overflow;
/// exactly i64 min/max parse successfully.
pub fn parse_i64(s: []u8) ?i64 {
    if (s.len == 0) {
        return null;
    }
    var neg: bool = false;
    var i: usize = 0;
    if (s[0] == 45) {
        neg = true;
        i = 1;
    }
    if (i >= s.len) {
        return null;
    }
    // Accumulate NEGATIVE (|i64 min| > i64 max, so the negative range covers
    // both extremes); overflow-check before each step, no signed-overflow UB.
    var min: i64 = (0 - 9223372036854775807) - 1;
    var acc: i64 = 0;
    while (i < s.len) : (i += 1) {
        var c: u8 = s[i];
        if (c < 48 or c > 57) {
            return null;
        }
        var d: i64 = @as(i64, c) - 48;
        if (acc < (min + d) / 10) {
            return null;
        }
        acc = acc * 10 - d;
    }
    if (neg) {
        return acc;
    }
    if (acc == min) {
        return null;
    }
    return 0 - acc;
}

/// Render `v` in decimal into a freshly-allocated `[]u8` (free it with
/// `free(a, result)`). Handles i64 min by working with the negative absolute
/// value (no negation UB).
pub fn fmt_i64(a: Allocator, v: i64) []u8 {
    // n = -|v| is always representable.
    var n: i64 = v;
    if (v > 0) {
        n = 0 - v;
    }
    var digits: usize = 1;
    var t: i64 = n;
    while (t <= 0 - 10) {
        t = t / 10;
        digits += 1;
    }
    var total: usize = digits;
    if (v < 0) {
        total += 1;
    }
    var out: []u8 = alloc(a, u8, total);
    if (v < 0) {
        out[0] = 45;
    }
    var m: i64 = n;
    var i: usize = total;
    var k: usize = 0;
    while (k < digits) : (k += 1) {
        i -= 1;
        var d: i64 = 0 - (m % 10);
        out[i] = @as(u8, 48 + d);
        m = m / 10;
    }
    return out;
}

/// Render `v` in lowercase hexadecimal (no leading zeros, "0" for 0) into a
/// freshly-allocated `[]u8` (free it with `free(a, result)`).
pub fn fmt_u64_hex(a: Allocator, v: u64) []u8 {
    var digits: usize = 1;
    var t: u64 = v;
    while (t >= 16) {
        t = t / 16;
        digits += 1;
    }
    var out: []u8 = alloc(a, u8, digits);
    var m: u64 = v;
    var i: usize = digits;
    var k: usize = 0;
    while (k < digits) : (k += 1) {
        i -= 1;
        var d: u64 = m % 16;
        if (d < 10) {
            out[i] = @as(u8, 48 + d);       // '0' + d
        } else {
            out[i] = @as(u8, 87 + d);       // 'a' + (d - 10)
        }
        m = m / 16;
    }
    return out;
}

/// Does `s` end with `suffix`? (Every string ends with the empty string.)
pub fn str_ends_with(s: []u8, suffix: []u8) bool {
    if (suffix.len > s.len) {
        return false;
    }
    var off: usize = s.len - suffix.len;
    var i: usize = 0;
    while (i < suffix.len) : (i += 1) {
        if (s[off + i] != suffix[i]) {
            return false;
        }
    }
    return true;
}

/// Index of the last byte equal to `c`, or -1 if absent (mirrors
/// `str_index_of`'s i32 return).
pub fn str_last_index_of(s: []u8, c: u8) i32 {
    var i: usize = s.len;
    while (i > 0) {
        i -= 1;
        if (s[i] == c) {
            return @as(i32, i);
        }
    }
    return 0 - 1;
}

/// How many bytes of `s` equal `c`?
pub fn str_count(s: []u8, c: u8) usize {
    var n: usize = 0;
    for (s) |b| {
        if (b == c) {
            n += 1;
        }
    }
    return n;
}

// --- StrBuilder — a growable byte/string builder -----------------------------

/// A growable byte buffer for assembling strings: `append` slices, single
/// bytes, or decimal integers, then `build` an exact-length `[]u8` copy.
/// Doubles its capacity like ArrayList; pass the same Allocator throughout.
pub const StrBuilder = struct {
    buf: []u8,
    count: usize,

    /// An empty builder (initial capacity 8 bytes).
    fn init(a: Allocator) Self {
        return Self{ .buf = alloc(a, u8, 8), .count = 0 };
    }

    // Grow (doubling) until at least `extra` more bytes fit.
    fn reserve(self: *Self, a: Allocator, extra: usize) void {
        if (self.count + extra <= self.buf.len) {
            return;
        }
        var cap: usize = self.buf.len * 2;
        while (cap < self.count + extra) {
            cap = cap * 2;
        }
        var grown: []u8 = alloc(a, u8, cap);
        var i: usize = 0;
        while (i < self.count) : (i += 1) {
            grown[i] = self.buf[i];
        }
        free(a, self.buf);
        self.buf = grown;
    }

    /// Append all bytes of `s`.
    fn append(self: *Self, a: Allocator, s: []u8) void {
        self.reserve(a, s.len);
        var i: usize = 0;
        while (i < s.len) : (i += 1) {
            self.buf[self.count + i] = s[i];
        }
        self.count += s.len;
    }

    /// Append a single byte.
    fn append_byte(self: *Self, a: Allocator, b: u8) void {
        self.reserve(a, 1);
        self.buf[self.count] = b;
        self.count += 1;
    }

    /// Append the decimal rendering of `v` (same format as `fmt_i64`).
    fn append_i64(self: *Self, a: Allocator, v: i64) void {
        var t: []u8 = fmt_i64(a, v);
        self.append(a, t);
        free(a, t);
    }

    /// Number of bytes appended so far.
    fn len(self: Self) usize {
        return self.count;
    }

    /// A freshly-allocated copy of exactly `len()` bytes (free it with
    /// `free(a, result)`); the builder stays usable.
    fn build(self: Self, a: Allocator) []u8 {
        var out: []u8 = alloc(a, u8, self.count);
        var i: usize = 0;
        while (i < self.count) : (i += 1) {
            out[i] = self.buf[i];
        }
        return out;
    }

    /// Free the builder's internal buffer.
    fn deinit(self: Self, a: Allocator) void {
        free(a, self.buf);
    }
};

// --- Deque(T) — a double-ended queue on a growable ring buffer --------------

/// A double-ended queue backed by a growable ring buffer: pushes and pops at
/// both ends are amortised O(1). Logical index `i` lives at physical slot
/// `(head + i) % buf.len`; when full the buffer doubles, re-linearising the
/// ring so the front lands back at slot 0.
pub fn Deque(comptime T: type) type {
    return struct {
        buf: []T,      // ring storage; capacity == buf.len
        head: usize,   // physical slot of the front element
        count: usize,  // live elements

        /// A fresh empty deque with a small initial capacity (4 slots).
        fn init(a: Allocator) Self {
            return Self{ .buf = alloc(a, T, 4), .head = 0, .count = 0 };
        }

        /// Release the backing buffer; the deque must not be used afterwards.
        fn deinit(self: Self, a: Allocator) void {
            free(a, self.buf);
        }

        /// Number of elements currently held.
        fn len(self: Self) usize {
            return self.count;
        }

        /// True when the deque holds no elements.
        fn is_empty(self: Self) bool {
            return self.count == 0;
        }

        // Double the capacity, copying the ring out in logical order so the
        // fresh buffer is linear again (head back at slot 0).
        fn grow(self: *Self, a: Allocator) void {
            var fresh: []T = alloc(a, T, self.buf.len * 2);
            var i: usize = 0;
            while (i < self.count) : (i += 1) {
                fresh[i] = self.buf[(self.head + i) % self.buf.len];
            }
            free(a, self.buf);
            self.buf = fresh;
            self.head = 0;
        }

        /// Append `v` at the back, doubling the capacity when full.
        fn push_back(self: *Self, a: Allocator, v: T) void {
            if (self.count == self.buf.len) {
                self.grow(a);
            }
            self.buf[(self.head + self.count) % self.buf.len] = v;
            self.count += 1;
        }

        /// Prepend `v` at the front, doubling the capacity when full.
        fn push_front(self: *Self, a: Allocator, v: T) void {
            if (self.count == self.buf.len) {
                self.grow(a);
            }
            self.head = (self.head + self.buf.len - 1) % self.buf.len;
            self.buf[self.head] = v;
            self.count += 1;
        }

        /// Remove and return the back element. Unchecked, like
        /// `ArrayList.get`: popping an empty deque is undefined (it returns
        /// whatever stale value the buffer holds) — guard with `is_empty()`.
        fn pop_back(self: *Self) T {
            self.count -= 1;
            return self.buf[(self.head + self.count) % self.buf.len];
        }

        /// Remove and return the front element. Unchecked: popping an empty
        /// deque is undefined (it returns whatever stale value the buffer
        /// holds) — guard with `is_empty()`.
        fn pop_front(self: *Self) T {
            var v: T = self.buf[self.head];
            self.head = (self.head + 1) % self.buf.len;
            self.count -= 1;
            return v;
        }

        /// The front element without removing it. Unchecked: undefined on an
        /// empty deque — guard with `is_empty()`.
        fn front(self: Self) T {
            return self.buf[self.head];
        }

        /// The back element without removing it. Unchecked: undefined on an
        /// empty deque — guard with `is_empty()`.
        fn back(self: Self) T {
            return self.buf[(self.head + self.count - 1) % self.buf.len];
        }
    };
}

// --- BitSet — a heap-backed dynamic bit set over []u64 words ----------------

// Words needed to hold `nbits` bits (ceil division by 64).
fn bitset_words_for(nbits: usize) usize {
    return (nbits + 63) / 64;
}

// The single-bit mask for bit `i` within its word.
fn bitset_mask(i: usize) u64 {
    return @as(u64, 1) << @as(u64, i % 64);
}

// Set-bit count of one word (Kernighan's clear-lowest-bit loop).
fn bitset_popcount(w: u64) usize {
    var x: u64 = w;
    var n: usize = 0;
    while (x != 0) : (x = x & (x - 1)) {
        n += 1;
    }
    return n;
}

fn bitset_min(a: usize, b: usize) usize {
    if (a < b) {
        return a;
    }
    return b;
}

/// A heap-backed dynamic bit set over `[]u64` words. Bit indexes are
/// range-guarded: `set`/`clear`/`toggle` with `i >= nbits` are no-ops and
/// `has` returns `false`. Create with `BitSet.init`, release with `deinit`.
pub const BitSet = struct {
    words: []u64,
    nbits: usize,

    /// A zeroed set holding `nbits` bits. Free it with `deinit(a)`.
    fn init(a: Allocator, nbits: usize) Self {
        var w: []u64 = alloc(a, u64, bitset_words_for(nbits));
        var i: usize = 0;
        while (i < w.len) : (i += 1) {
            w[i] = 0;
        }
        return Self{ .words = w, .nbits = nbits };
    }

    /// Set bit `i` to 1 (no-op when `i >= nbits`).
    fn set(self: *Self, i: usize) void {
        if (i >= self.nbits) {
            return;
        }
        self.words[i / 64] = self.words[i / 64] | bitset_mask(i);
    }

    /// Clear bit `i` to 0 (no-op when `i >= nbits`).
    fn clear(self: *Self, i: usize) void {
        if (i >= self.nbits) {
            return;
        }
        self.words[i / 64] = self.words[i / 64] & ~bitset_mask(i);
    }

    /// Flip bit `i` (no-op when `i >= nbits`).
    fn toggle(self: *Self, i: usize) void {
        if (i >= self.nbits) {
            return;
        }
        self.words[i / 64] = self.words[i / 64] ^ bitset_mask(i);
    }

    /// Is bit `i` set? Returns `false` when `i >= nbits`.
    fn has(self: Self, i: usize) bool {
        if (i >= self.nbits) {
            return false;
        }
        return (self.words[i / 64] & bitset_mask(i)) != 0;
    }

    /// Number of set bits (population count over all words).
    fn count(self: Self) usize {
        var n: usize = 0;
        var i: usize = 0;
        while (i < self.words.len) : (i += 1) {
            n += bitset_popcount(self.words[i]);
        }
        return n;
    }

    /// In-place union: `self = self | other`. Both sets are assumed to have
    /// equal `nbits`; with mismatched sizes only the shorter word span is
    /// combined (no out-of-range access).
    fn union_with(self: *Self, other: Self) void {
        var n: usize = bitset_min(self.words.len, other.words.len);
        var i: usize = 0;
        while (i < n) : (i += 1) {
            self.words[i] = self.words[i] | other.words[i];
        }
    }

    /// In-place intersection: `self = self & other`. Both sets are assumed to
    /// have equal `nbits`; with mismatched sizes only the shorter word span is
    /// combined (no out-of-range access).
    fn intersect_with(self: *Self, other: Self) void {
        var n: usize = bitset_min(self.words.len, other.words.len);
        var i: usize = 0;
        while (i < n) : (i += 1) {
            self.words[i] = self.words[i] & other.words[i];
        }
    }

    /// In-place difference: `self = self & ~other`. Both sets are assumed to
    /// have equal `nbits`; with mismatched sizes only the shorter word span is
    /// combined (no out-of-range access).
    fn difference_with(self: *Self, other: Self) void {
        var n: usize = bitset_min(self.words.len, other.words.len);
        var i: usize = 0;
        while (i < n) : (i += 1) {
            self.words[i] = self.words[i] & ~other.words[i];
        }
    }

    /// `true` when no bit is set.
    fn is_empty(self: Self) bool {
        var i: usize = 0;
        while (i < self.words.len) : (i += 1) {
            if (self.words[i] != 0) {
                return false;
            }
        }
        return true;
    }

    /// The number of bits this set holds (its `nbits`).
    fn capacity(self: Self) usize {
        return self.nbits;
    }

    /// Clear every bit to 0 (capacity unchanged).
    fn clear_all(self: *Self) void {
        var i: usize = 0;
        while (i < self.words.len) : (i += 1) {
            self.words[i] = 0;
        }
    }

    /// Release the backing words. The set must not be used afterwards.
    fn deinit(self: Self, a: Allocator) void {
        free(a, self.words);
    }
};

// --- rng — deterministic PRNG + shuffle --------------------------------------

/// A deterministic pseudo-random number generator (xorshift64*). Seed it once
/// with `Rng.init(seed)`; the same seed always yields the same sequence.
pub const Rng = struct {
    state: u64,

    /// Create a generator from `seed`. xorshift64* requires a nonzero state,
    /// so seed 0 is mapped to the fixed constant 88172645463325252.
    pub fn init(seed: u64) Rng {
        var s: u64 = seed;
        if (s == 0) {
            s = 88172645463325252;
        }
        return Rng{ .state = s };
    }

    /// Next raw 64-bit value (xorshift64*: three xor-shifts update the state,
    /// the output is `state * 2685821657736338717` — u64 arithmetic wraps
    /// modulo 2^64 by definition, so plain `*` is the wrapping multiply).
    pub fn next_u64(self: *Rng) u64 {
        var x: u64 = self.state;
        x = x ^ (x >> 12);
        x = x ^ (x << 25);
        x = x ^ (x >> 27);
        self.state = x;
        return x * 2685821657736338717;
    }

    /// Next value in `[0, n)`, by modulo — the small modulo bias for `n` not
    /// a power of two is accepted by design. `n == 0` returns 0 (no draw is
    /// well-defined below zero); `n == 1` always returns 0.
    pub fn next_below(self: *Rng, n: u64) u64 {
        if (n == 0) {
            return 0;
        }
        return self.next_u64() % n;
    }

    /// Next value in the inclusive range `[lo, hi]`. A degenerate range
    /// (`lo >= hi`) returns `lo` without consuming a draw. The span is
    /// computed in u64 so no signed overflow occurs even for extreme bounds;
    /// the full i64 range maps one raw draw via a two's-complement cast.
    pub fn next_i64_in(self: *Rng, lo: i64, hi: i64) i64 {
        if (lo >= hi) {
            return lo;
        }
        var span: u64 = @as(u64, hi) - @as(u64, lo);
        var bound: u64 = span + 1;
        if (bound == 0) {
            return @as(i64, self.next_u64());
        }
        var off: u64 = self.next_below(bound);
        return @as(i64, @as(u64, lo) + off);
    }
};

/// Shuffle `xs` in place with the Fisher-Yates algorithm, drawing positions
/// from `r` (one `next_below(i + 1)` per element, from the last index down to
/// 1). Deterministic for a given seed; empty and single-element slices are
/// no-ops that consume no draws.
pub fn shuffle(comptime T: type, r: *Rng, xs: []T) void {
    var i: usize = xs.len;
    while (i > 1) {
        i -= 1;
        var j: usize = @as(usize, r.next_below(@as(u64, i) + 1));
        var tmp: T = xs[i];
        xs[i] = xs[j];
        xs[j] = tmp;
    }
}

// --- numtext — float/number text conversions + ASCII case utils --------------
//
// f64 parse/format, overflow-safe u64 parse, u64/padded-i64 formatting, and
// ASCII case mapping. Formatting is fixed-point over double arithmetic: the
// fraction is rounded half-up on the BINARY value, so decimal-looking ties
// follow the f64 representation (fmt_f64(2.675, 2) is "2.67" because the
// nearest double to 2.675 is 2.67499999999999982...).

// 10 raised to `n` (n >= 0) in f64, by squaring. Exact for n <= 22 (every
// power of 10 up to 10^22 is exactly representable); larger n round, and far
// larger n overflow to +inf — exactly what parse_f64's scaling wants.
fn numtext_pow10_f64(n: i64) f64 {
    var r: f64 = 1.0;
    var b: f64 = 10.0;
    var e: i64 = n;
    while (e > 0) : (e = e / 2) {
        if (e % 2 == 1) {
            r = r * b;
        }
        if (e > 1) {
            b = b * b;
        }
    }
    return r;
}

// ASCII lower-fold of one byte: 'A'..'Z' -> 'a'..'z', all others unchanged.
fn numtext_lower_byte(c: u8) u8 {
    if (c >= 65 and c <= 90) {
        return c + 32;
    }
    return c;
}

// Fixed-point rendering of a finite w >= 2^63 (always an exact integer:
// every double >= 2^53 is integral). Splits w into m0 * 2^k by exact
// halving, seeds a little-endian decimal digit array from m0 and doubles it
// k times (schoolbook carry), then emits sign + digits + '.' + d zeros.
fn numtext_fmt_big(a: Allocator, w: f64, d: i64, neg: bool) []u8 {
    var h: f64 = w;
    var k: i64 = 0;
    while (h >= 9223372036854775808.0) : (k += 1) {
        h = h / 2.0;        // exponent decrement: always exact
    }
    var m0: u64 = @as(u64, h);
    // Largest double < 2^1024 < 10^309, so 340 digit slots are plenty.
    var dig: []u8 = alloc(a, u8, 340);
    var nd: usize = 0;
    while (m0 > 0) : (m0 = m0 / 10) {
        dig[nd] = @as(u8, m0 % 10);
        nd += 1;
    }
    var t: i64 = 0;
    while (t < k) : (t += 1) {
        var carry: u8 = 0;
        var j: usize = 0;
        while (j < nd) : (j += 1) {
            var x: u8 = dig[j] * 2 + carry;     // <= 19, fits u8
            dig[j] = x % 10;
            carry = x / 10;
        }
        if (carry > 0) {
            dig[nd] = carry;
            nd += 1;
        }
    }
    var total: usize = nd;
    if (neg) {
        total += 1;
    }
    if (d > 0) {
        total += 1 + @as(usize, d);
    }
    var out: []u8 = alloc(a, u8, total);
    var i: usize = 0;
    if (neg) {
        out[0] = 45;        // '-'
        i = 1;
    }
    var r: usize = nd;
    while (r > 0) {
        r -= 1;
        out[i] = 48 + dig[r];
        i += 1;
    }
    if (d > 0) {
        out[i] = 46;        // '.'
        i += 1;
        var z: i64 = 0;
        while (z < d) : (z += 1) {
            out[i] = 48;    // '0'
            i += 1;
        }
    }
    free(a, dig);
    return out;
}

/// Parse a decimal floating-point string into an `f64`, or null on any
/// deviation from `[+|-] digits [. digits] [(e|E) [+|-] digits]` (at least
/// one mantissa digit somewhere; exponent digits required after `e`). So
/// "0.5", "-3.25", "+2.5", "5.", ".5", "1e3", "2.5E-2" parse; "", ".", "-",
/// "e3", "1e", "1e+", "1.2.3", " 1", "inf" and "nan" do not. Digits
/// accumulate in f64 (mantissas past ~15 significant digits round); the
/// exponent scales by an exact power of 10 (multiply for positive, divide
/// for negative), so values overflow to +/-inf and underflow to 0 instead
/// of failing.
pub fn parse_f64(s: []u8) ?f64 {
    var i: usize = 0;
    var neg: bool = false;
    if (i < s.len and (s[i] == 43 or s[i] == 45)) {     // '+' / '-'
        neg = s[i] == 45;
        i += 1;
    }
    var m: f64 = 0.0;
    var any: bool = false;
    while (i < s.len) {
        var c: u8 = s[i];
        if (c < 48 or c > 57) {
            break;
        }
        m = m * 10.0 + @as(f64, c - 48);
        any = true;
        i += 1;
    }
    var fracd: i64 = 0;
    if (i < s.len and s[i] == 46) {                     // '.'
        i += 1;
        while (i < s.len) {
            var c2: u8 = s[i];
            if (c2 < 48 or c2 > 57) {
                break;
            }
            m = m * 10.0 + @as(f64, c2 - 48);
            fracd += 1;
            any = true;
            i += 1;
        }
    }
    if (!any) {
        return null;
    }
    var exp: i64 = 0;
    if (i < s.len and (s[i] == 101 or s[i] == 69)) {    // 'e' / 'E'
        i += 1;
        var eneg: bool = false;
        if (i < s.len and (s[i] == 43 or s[i] == 45)) {
            eneg = s[i] == 45;
            i += 1;
        }
        var eany: bool = false;
        while (i < s.len) {
            var c3: u8 = s[i];
            if (c3 < 48 or c3 > 57) {
                break;
            }
            if (exp < 100000) {     // clamp: anything larger is +/-inf or 0 anyway
                exp = exp * 10 + @as(i64, c3 - 48);
            }
            eany = true;
            i += 1;
        }
        if (!eany) {
            return null;
        }
        if (eneg) {
            exp = 0 - exp;
        }
    }
    if (i != s.len) {
        return null;
    }
    var r: f64 = m;
    if (m != 0.0) {                 // skip scaling 0 (avoids 0 * inf = NaN)
        var e10: i64 = exp - fracd;
        if (e10 > 0) {
            r = m * numtext_pow10_f64(e10);
        }
        if (e10 < 0) {
            r = m / numtext_pow10_f64(0 - e10);
        }
    }
    if (neg) {
        return 0.0 - r;
    }
    return r;
}

/// Render `v` in unsigned decimal into a freshly-allocated `[]u8` (free it
/// with `free(a, result)`). The full u64 range, "0" for 0.
pub fn fmt_u64(a: Allocator, v: u64) []u8 {
    var digits: usize = 1;
    var t: u64 = v;
    while (t >= 10) {
        t = t / 10;
        digits += 1;
    }
    var out: []u8 = alloc(a, u8, digits);
    var m: u64 = v;
    var i: usize = digits;
    var k: usize = 0;
    while (k < digits) : (k += 1) {
        i -= 1;
        out[i] = @as(u8, 48 + (m % 10));
        m = m / 10;
    }
    return out;
}

/// Render `v` as fixed-point decimal with exactly `decimals` fraction digits
/// (clamped into 0..17; 0 means no '.') into a freshly-allocated `[]u8`
/// (free it with `free(a, result)`). The fraction is rounded half-up on the
/// binary value of `v` — see the module note on 2.675. NaN renders as
/// "nan", infinities as "inf"/"-inf"; every finite double works, including
/// magnitudes >= 2^63 (rendered exactly via decimal doubling). -0.0 renders
/// unsigned ("0.0").
pub fn fmt_f64(a: Allocator, v: f64, decimals: i64) []u8 {
    var d: i64 = decimals;
    if (d < 0) {
        d = 0;
    }
    if (d > 17) {
        d = 17;
    }
    if (v != v) {
        return str_concat(a, "nan", "");        // fresh copy: caller frees
    }
    var neg: bool = v < 0.0;
    var w: f64 = v;
    if (neg) {
        w = 0.0 - v;
    }
    if (w > 0.0 and w / 2.0 == w) {             // only +inf survives both
        if (neg) {
            return str_concat(a, "-inf", "");
        }
        return str_concat(a, "inf", "");
    }
    if (w >= 9223372036854775808.0) {           // 2^63: out of u64-truncation range
        return numtext_fmt_big(a, w, d, neg);
    }
    var ip: u64 = @as(u64, w);
    var fp: f64 = w - @as(f64, ip);             // exact: low bits of w
    var p10: u64 = 1;
    var k: i64 = 0;
    while (k < d) : (k += 1) {
        p10 = p10 * 10;                         // <= 10^17, fits u64
    }
    var fr: u64 = @as(u64, fp * @as(f64, p10) + 0.5);
    if (fr >= p10) {                            // rounded up into the next unit
        fr = 0;
        ip += 1;
    }
    var ips: []u8 = fmt_u64(a, ip);
    var total: usize = ips.len;
    if (neg) {
        total += 1;
    }
    if (d > 0) {
        total += 1 + @as(usize, d);
    }
    var out: []u8 = alloc(a, u8, total);
    var i: usize = 0;
    if (neg) {
        out[0] = 45;                            // '-'
        i = 1;
    }
    var j: usize = 0;
    while (j < ips.len) : (j += 1) {
        out[i] = ips[j];
        i += 1;
    }
    free(a, ips);
    if (d > 0) {
        out[i] = 46;                            // '.'
        i += 1;
        var frs: []u8 = fmt_u64(a, fr);
        var z: usize = @as(usize, d) - frs.len; // fr < p10 -> at most d digits
        var zi: usize = 0;
        while (zi < z) : (zi += 1) {
            out[i] = 48;                        // '0' left-pad
            i += 1;
        }
        var fi: usize = 0;
        while (fi < frs.len) : (fi += 1) {
            out[i] = frs[fi];
            i += 1;
        }
        free(a, frs);
    }
    return out;
}

/// Parse an unsigned decimal integer (digits only — no sign, no spaces).
/// Returns null on an empty string, any non-digit byte, or u64 overflow;
/// exactly 18446744073709551615 (u64 max) parses successfully.
pub fn parse_u64(s: []u8) ?u64 {
    if (s.len == 0) {
        return null;
    }
    var acc: u64 = 0;
    var i: usize = 0;
    while (i < s.len) : (i += 1) {
        var c: u8 = s[i];
        if (c < 48 or c > 57) {
            return null;
        }
        var d: u64 = @as(u64, c - 48);
        // u64 max = 18446744073709551615 = 1844674407370955161 * 10 + 5.
        if (acc > 1844674407370955161) {
            return null;
        }
        if (acc == 1844674407370955161 and d > 5) {
            return null;
        }
        acc = acc * 10 + d;
    }
    return acc;
}

/// Render `v` in decimal padded to at least `width` bytes into a
/// freshly-allocated `[]u8` (free it with `free(a, result)`). `zero_pad`
/// false pads with leading spaces ("   -42"); true pads with zeros AFTER
/// the sign ("-00042"). When the plain rendering is already `width` or
/// longer (including any `width <= 0`) it is returned unpadded.
pub fn fmt_i64_pad(a: Allocator, v: i64, width: i64, zero_pad: bool) []u8 {
    var body: []u8 = fmt_i64(a, v);
    if (@as(i64, body.len) >= width) {
        return body;
    }
    var total: usize = @as(usize, width);
    var out: []u8 = alloc(a, u8, total);
    var i: usize = 0;
    var src: usize = 0;
    if (zero_pad and v < 0) {
        out[0] = 45;            // '-' first, zeros after it
        i = 1;
        src = 1;                // skip body's own '-'
    }
    var pad: u8 = 32;           // ' '
    if (zero_pad) {
        pad = 48;               // '0'
    }
    var stop: usize = total - (body.len - src);
    while (i < stop) : (i += 1) {
        out[i] = pad;
    }
    while (src < body.len) : (src += 1) {
        out[i] = body[src];
        i += 1;
    }
    free(a, body);
    return out;
}

/// ASCII-lowercase copy of `s` ('A'..'Z' -> 'a'..'z', everything else byte-
/// for-byte, so UTF-8 multibyte sequences pass through untouched) in a
/// freshly-allocated `[]u8` (free it with `free(a, result)`).
pub fn to_lower(a: Allocator, s: []u8) []u8 {
    var out: []u8 = alloc(a, u8, s.len);
    var i: usize = 0;
    while (i < s.len) : (i += 1) {
        out[i] = numtext_lower_byte(s[i]);
    }
    return out;
}

/// ASCII-uppercase copy of `s` ('a'..'z' -> 'A'..'Z', everything else byte-
/// for-byte) in a freshly-allocated `[]u8` (free it with `free(a, result)`).
pub fn to_upper(a: Allocator, s: []u8) []u8 {
    var out: []u8 = alloc(a, u8, s.len);
    var i: usize = 0;
    while (i < s.len) : (i += 1) {
        var c: u8 = s[i];
        if (c >= 97 and c <= 122) {
            c -= 32;
        }
        out[i] = c;
    }
    return out;
}

/// Are `x` and `y` equal ignoring ASCII letter case? Allocation-free
/// per-byte comparison after lower-folding 'A'..'Z' only — non-letter bytes
/// must match exactly ('@' never equals '`').
pub fn eq_ignore_case(x: []u8, y: []u8) bool {
    if (x.len != y.len) {
        return false;
    }
    var i: usize = 0;
    while (i < x.len) : (i += 1) {
        if (numtext_lower_byte(x[i]) != numtext_lower_byte(y[i])) {
            return false;
        }
    }
    return true;
}


// --- baseenc — base64 + hex codecs over []u8 ---------------------------------
//
// Allocating encoders/decoders for RFC 4648 standard base64 (alphabet
// A-Z a-z 0-9 + /, `=` padding) and lowercase hex.
//
// Error convention: `![]u8` is not expressible (error unions wrap NAMED
// types only), so the decoders return an EMPTY slice on any invalid input.
// An empty result is also the correct decoding of an empty input, so a
// caller that must tell "error" apart from "decoded nothing" should check
// the input first (`text.len != 0` plus `b64_decoded_len(text) == 0` for
// base64, odd `text.len` for hex). Every result — including the empty one —
// is freshly allocated and safe to release with `free(a, result)`.

// A freshly-allocated zero-length slice (the codecs' empty/error result;
// never a static literal, so the caller can always `free` it).
fn baseenc_empty(a: Allocator) []u8 {
    return alloc(a, u8, 0);
}

// The base64 character for a 6-bit value 0..63 (standard alphabet).
fn baseenc_b64_char(v: u64) u8 {
    if (v < 26) {
        return @as(u8, 65 + v);      // 'A' + v
    }
    if (v < 52) {
        return @as(u8, 71 + v);      // 'a' + (v - 26)
    }
    if (v < 62) {
        return @as(u8, v - 4);       // '0' + (v - 52)
    }
    if (v == 62) {
        return 43;                   // '+'
    }
    return 47;                       // '/'
}

// The 6-bit value of a base64 character, or -1 when it is not in the
// standard alphabet ('=' is padding, not alphabet: it yields -1 too).
fn baseenc_b64_val(c: u8) i32 {
    if (c >= 65 and c <= 90) {       // 'A'..'Z'
        return @as(i32, c) - 65;
    }
    if (c >= 97 and c <= 122) {      // 'a'..'z'
        return @as(i32, c) - 71;
    }
    if (c >= 48 and c <= 57) {       // '0'..'9'
        return @as(i32, c) + 4;
    }
    if (c == 43) {                   // '+'
        return 62;
    }
    if (c == 47) {                   // '/'
        return 63;
    }
    return 0 - 1;
}

/// Base64-encoded length for `n` input bytes: 4 output characters per
/// started 3-byte group (RFC 4648 with `=` padding) — 0 -> 0, 1..3 -> 4,
/// 4..6 -> 8, and so on.
pub fn b64_encoded_len(n: usize) usize {
    return (n + 2) / 3 * 4;
}

/// Decoded byte count of a base64 `text`, judged by length and trailing
/// padding only: `text.len / 4 * 3` minus one per trailing `=` (at most
/// two). Returns 0 when `text.len` is not a positive multiple of 4. Note
/// this helper does not validate the alphabet: `b64_decode` can still
/// reject a `text` this function sizes.
pub fn b64_decoded_len(text: []u8) usize {
    if (text.len == 0 or text.len % 4 != 0) {
        return 0;
    }
    var n: usize = text.len / 4 * 3;
    if (text[text.len - 1] == 61) {          // '='
        n -= 1;
        if (text[text.len - 2] == 61) {
            n -= 1;
        }
    }
    return n;
}

/// Encode `data` as RFC 4648 standard base64 (alphabet A-Z a-z 0-9 + /,
/// `=` padding) into a freshly-allocated `[]u8` of exactly
/// `b64_encoded_len(data.len)` bytes — free it with `free(a, result)`.
/// Empty input yields an empty (zero-length, still freeable) result.
pub fn b64_encode(a: Allocator, data: []u8) []u8 {
    var out: []u8 = alloc(a, u8, b64_encoded_len(data.len));
    var i: usize = 0;
    var o: usize = 0;
    while (i + 3 <= data.len) : (i += 3) {
        var n: u64 = (@as(u64, data[i]) << 16) | (@as(u64, data[i + 1]) << 8) | @as(u64, data[i + 2]);
        out[o] = baseenc_b64_char((n >> 18) & 63);
        out[o + 1] = baseenc_b64_char((n >> 12) & 63);
        out[o + 2] = baseenc_b64_char((n >> 6) & 63);
        out[o + 3] = baseenc_b64_char(n & 63);
        o += 4;
    }
    var rem: usize = data.len - i;
    if (rem == 1) {
        var n1: u64 = @as(u64, data[i]) << 16;
        out[o] = baseenc_b64_char((n1 >> 18) & 63);
        out[o + 1] = baseenc_b64_char((n1 >> 12) & 63);
        out[o + 2] = 61;                     // '='
        out[o + 3] = 61;
    }
    if (rem == 2) {
        var n2: u64 = (@as(u64, data[i]) << 16) | (@as(u64, data[i + 1]) << 8);
        out[o] = baseenc_b64_char((n2 >> 18) & 63);
        out[o + 1] = baseenc_b64_char((n2 >> 12) & 63);
        out[o + 2] = baseenc_b64_char((n2 >> 6) & 63);
        out[o + 3] = 61;                     // '='
    }
    return out;
}

/// Decode RFC 4648 standard base64 into a freshly-allocated `[]u8` — free
/// it with `free(a, result)`. Strict: `text.len` must be a multiple of 4,
/// every character must come from the standard alphabet, and `=` may appear
/// only as one or two FINAL padding characters; ANY violation returns an
/// empty slice (see the module-header error convention — `![]u8` is not
/// expressible). The unused low bits of a padded final group are ignored,
/// not required to be zero. `""` decodes to an empty slice (success).
pub fn b64_decode(a: Allocator, text: []u8) []u8 {
    if (text.len == 0) {
        return baseenc_empty(a);
    }
    if (text.len % 4 != 0) {
        return baseenc_empty(a);
    }
    // Padding: at most the final two characters (text.len >= 4 here).
    var pad: usize = 0;
    if (text[text.len - 1] == 61) {          // '='
        pad = 1;
        if (text[text.len - 2] == 61) {
            pad = 2;
        }
    }
    // Every non-padding character must be in the alphabet; this also
    // rejects '=' anywhere before the final padding run.
    var body: usize = text.len - pad;
    var k: usize = 0;
    while (k < body) : (k += 1) {
        if (baseenc_b64_val(text[k]) < 0) {
            return baseenc_empty(a);
        }
    }
    var out: []u8 = alloc(a, u8, text.len / 4 * 3 - pad);
    var g: usize = 0;
    var o: usize = 0;
    while (g + 4 < text.len) : (g += 4) {    // every group but the last
        var n: u64 = (@as(u64, baseenc_b64_val(text[g])) << 18) | (@as(u64, baseenc_b64_val(text[g + 1])) << 12) | (@as(u64, baseenc_b64_val(text[g + 2])) << 6) | @as(u64, baseenc_b64_val(text[g + 3]));
        out[o] = @as(u8, (n >> 16) & 255);
        out[o + 1] = @as(u8, (n >> 8) & 255);
        out[o + 2] = @as(u8, n & 255);
        o += 3;
    }
    // Last group (g == text.len - 4): padding characters contribute 0 bits.
    var c2: u64 = 0;
    var c3: u64 = 0;
    if (pad < 2) {
        c2 = @as(u64, baseenc_b64_val(text[g + 2]));
    }
    if (pad < 1) {
        c3 = @as(u64, baseenc_b64_val(text[g + 3]));
    }
    var last: u64 = (@as(u64, baseenc_b64_val(text[g])) << 18) | (@as(u64, baseenc_b64_val(text[g + 1])) << 12) | (c2 << 6) | c3;
    out[o] = @as(u8, (last >> 16) & 255);
    if (pad < 2) {
        out[o + 1] = @as(u8, (last >> 8) & 255);
    }
    if (pad < 1) {
        out[o + 2] = @as(u8, last & 255);
    }
    return out;
}

// The lowercase hex character for a 4-bit value 0..15.
fn baseenc_hex_digit(v: u8) u8 {
    if (v < 10) {
        return 48 + v;               // '0' + v
    }
    return 87 + v;                   // 'a' + (v - 10)
}

// The value of a hex character (accepts 0-9, a-f and A-F), or -1 when it
// is not a hex digit.
fn baseenc_hex_val(c: u8) i32 {
    if (c >= 48 and c <= 57) {       // '0'..'9'
        return @as(i32, c) - 48;
    }
    if (c >= 97 and c <= 102) {      // 'a'..'f'
        return @as(i32, c) - 87;
    }
    if (c >= 65 and c <= 70) {       // 'A'..'F'
        return @as(i32, c) - 55;
    }
    return 0 - 1;
}

/// Encode `data` as lowercase hex, two characters per byte, into a
/// freshly-allocated `[]u8` of `data.len * 2` bytes — free it with
/// `free(a, result)`. Empty input yields an empty (freeable) result.
pub fn hex_encode(a: Allocator, data: []u8) []u8 {
    var out: []u8 = alloc(a, u8, data.len * 2);
    var i: usize = 0;
    while (i < data.len) : (i += 1) {
        out[i * 2] = baseenc_hex_digit(data[i] / 16);
        out[i * 2 + 1] = baseenc_hex_digit(data[i] % 16);
    }
    return out;
}

/// Decode a hex string (case-insensitive: 0-9, a-f, A-F) into a
/// freshly-allocated `[]u8` of `text.len / 2` bytes — free it with
/// `free(a, result)`. An odd-length input or any non-hex character returns
/// an empty slice (see the module-header error convention — `![]u8` is not
/// expressible). `""` decodes to an empty slice (success).
pub fn hex_decode(a: Allocator, text: []u8) []u8 {
    if (text.len % 2 != 0) {
        return baseenc_empty(a);
    }
    var k: usize = 0;
    while (k < text.len) : (k += 1) {
        if (baseenc_hex_val(text[k]) < 0) {
            return baseenc_empty(a);
        }
    }
    var out: []u8 = alloc(a, u8, text.len / 2);
    var j: usize = 0;
    while (j < out.len) : (j += 1) {
        out[j] = @as(u8, baseenc_hex_val(text[j * 2]) * 16 + baseenc_hex_val(text[j * 2 + 1]));
    }
    return out;
}

// --- hashes — checksums & non-crypto hashes over []u8 ------------------------
//
// One-shot digests of a byte slice, plus a streaming Crc32. All arithmetic is
// unsigned (u32 wraps mod 2^32, u64 mod 2^64 — C-defined), so there is no
// overflow UB anywhere in this module. None of these are cryptographic.

// One step of reflected CRC-32 (IEEE 802.3, polynomial 0xEDB88320 =
// 3988292384): xor the byte into the low bits, then 8 conditional
// shift-and-xor rounds. Operates on the *internal* (pre-final-xor) state.
fn hashes_crc32_byte(crc: u32, b: u8) u32 {
    var c: u32 = crc ^ @as(u32, b);
    var k: i64 = 0;
    while (k < 8) : (k += 1) {
        if ((c & 1) != 0) {
            c = (c >> 1) ^ 3988292384;  // 0xEDB88320
        } else {
            c = c >> 1;
        }
    }
    return c;
}

/// CRC-32 of `data` (IEEE 802.3, the zlib/PNG/Ethernet checksum): reflected,
/// polynomial 0xEDB88320, init and final xor 0xFFFFFFFF. Empty input -> 0;
/// `crc32("123456789") == 3421780262` (0xCBF43926). Uses a 16-entry nibble
/// table built locally (no globals); equals the streaming `Crc32` result.
pub fn crc32(data: []u8) u32 {
    // table[n] = the CRC state reached by shifting nibble n through 4 rounds.
    var table: [16]u32 = [16]u32{ 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0 };
    var n: usize = 0;
    while (n < 16) : (n += 1) {
        var e: u32 = @as(u32, n);
        var k: i64 = 0;
        while (k < 4) : (k += 1) {
            if ((e & 1) != 0) {
                e = (e >> 1) ^ 3988292384;  // 0xEDB88320
            } else {
                e = e >> 1;
            }
        }
        table[n] = e;
    }
    var c: u32 = 4294967295;                // ~0: initial state
    for (data) |b| {
        c = c ^ @as(u32, b);
        c = table[@as(usize, c & 15)] ^ (c >> 4);   // low nibble
        c = table[@as(usize, c & 15)] ^ (c >> 4);   // high nibble
    }
    return c ^ 4294967295;                  // final xor
}

/// Streaming CRC-32 (same parameters as `crc32`): feed any number of `update`
/// slices, then read the digest with `final`. `final` does not consume the
/// state, so more `update`s may follow (the digest then covers all bytes fed
/// so far). `Crc32.init()` -> `update(...)` -> `final()` over the
/// concatenation of the updates equals one-shot `crc32` of the whole input.
pub const Crc32 = struct {
    state: u32,   // internal state = digest-so-far ^ 0xFFFFFFFF

    /// A fresh hasher (no bytes fed; `final()` of it is 0, like `crc32("")`).
    fn init() Self {
        return Self{ .state = 4294967295 };
    }

    /// Feed all bytes of `data` (an empty slice is a no-op).
    fn update(self: *Self, data: []u8) void {
        var c: u32 = self.state;
        for (data) |b| {
            c = hashes_crc32_byte(c, b);
        }
        self.state = c;
    }

    /// The CRC-32 of every byte fed so far (applies the final xor; the
    /// hasher itself is left untouched).
    fn final(self: Self) u32 {
        return self.state ^ 4294967295;
    }
};

/// FNV-1a 32-bit hash of `data`: h = 2166136261, then per byte
/// h = (h ^ byte) * 16777619, wrapping mod 2^32. Empty input returns the
/// offset basis 2166136261. Fast and well-distributed for hash tables;
/// not cryptographic.
pub fn fnv1a32(data: []u8) u32 {
    var h: u32 = 2166136261;        // FNV-1a 32-bit offset basis
    for (data) |b| {
        h = h ^ @as(u32, b);
        h = h * 16777619;           // FNV 32-bit prime, wraps mod 2^32
    }
    return h;
}

/// FNV-1a 64-bit hash of `data`: h = 14695981039346656037, then per byte
/// h = (h ^ byte) * 1099511628211, wrapping mod 2^64. Empty input returns
/// the offset basis 14695981039346656037 (0xcbf29ce484222325).
pub fn fnv1a64(data: []u8) u64 {
    // The offset basis exceeds the i64 literal range, so build it by u64
    // wraparound: 0 - 3750763034362895579 == 2^64 - 3750763034362895579
    //           == 14695981039346656037.
    var h: u64 = 0;
    h = h - 3750763034362895579;
    for (data) |b| {
        h = h ^ @as(u64, b);
        h = h * 1099511628211;      // FNV 64-bit prime, wraps mod 2^64
    }
    return h;
}

/// Adler-32 checksum of `data` (the zlib checksum, RFC 1950): two running
/// sums mod 65521 — `s1` of the bytes (starting at 1), `s2` of the `s1`
/// values — packed as `(s2 << 16) | s1`. Empty input -> 1;
/// `adler32("Wikipedia") == 300286872` (0x11E60398).
pub fn adler32(data: []u8) u32 {
    var s1: u32 = 1;
    var s2: u32 = 0;
    for (data) |b| {
        s1 = (s1 + @as(u32, b)) % 65521;
        s2 = (s2 + s1) % 65521;
    }
    return (s2 << 16) | s1;
}

/// djb2 string hash (Bernstein): h = 5381, then per byte h = h * 33 + byte,
/// wrapping mod 2^32. Empty input -> 5381; `djb2("a") == 177670`
/// (5381 * 33 + 97). The classic additive variant (not the xor variant).
pub fn djb2(data: []u8) u32 {
    var h: u32 = 5381;
    for (data) |b| {
        h = h * 33 + @as(u32, b);
    }
    return h;
}

// --- strops — splitting / joining / trimming / replacing over `[]u8` ---------
//
// Splitting is iterator-style (no closures in the language, and `?[]u8` is
// not expressible, so iteration is a `next(*Self) bool` + `current(Self) []u8`
// two-call protocol). Subslice-returning functions (`trim*`, `current`) are
// ZERO-COPY views into the input — the input must outlive them and there is
// nothing to free. Allocating functions (`split_collect`, `join`, `replace`)
// return fresh storage: free results with `free(a, result)` and collected
// span lists with `.deinit(a)`.

// Is `b` ASCII whitespace? (space 32, tab 9, LF 10, CR 13)
fn strops_is_space(b: u8) bool {
    return b == 32 or b == 9 or b == 10 or b == 13;
}

// Does `needle` occur in `s` starting exactly at index `at`?
fn strops_match_at(s: []u8, needle: []u8, at: usize) bool {
    if (at + needle.len > s.len) {
        return false;
    }
    var k: usize = 0;
    while (k < needle.len) : (k += 1) {
        if (s[at + k] != needle[k]) {
            return false;
        }
    }
    return true;
}

// First index i >= start with s[i .. i + needle.len] == needle, or -1 if
// absent. Requires needle.len >= 1 (callers guard the empty-needle case).
fn strops_find_from(s: []u8, needle: []u8, start: usize) i64 {
    if (needle.len > s.len) {
        return 0 - 1;
    }
    var i: usize = start;
    var last: usize = s.len - needle.len;
    while (i <= last) : (i += 1) {
        if (strops_match_at(s, needle, i)) {
            return @as(i64, i);
        }
    }
    return 0 - 1;
}

/// An `(off, len)` pair naming the subslice `src[off .. off + len]` of some
/// source string. `split_collect` returns these instead of slices because a
/// generic type argument must be a bare type name — `ArrayList([]u8)` is not
/// expressible. Recover the field text with `src[p.off .. p.off + p.len]`,
/// or hand the whole list to `join`.
pub const SpanPair = struct {
    off: usize,
    len: usize,
};

/// Iterator over the fields of a string split on a single separator byte.
/// Protocol (the language has no closures and `?[]u8` is not expressible,
/// so iteration is two calls):
///
///     var it: Splitter = split_init("a,b", 44);
///     while (it.next()) {
///         var field: []u8 = it.current();
///     }
///
/// `next` advances to the following field, returning `true` while one was
/// produced; `current` is the field most recently advanced to, a zero-copy
/// subslice of the source. Empty fields between consecutive separators ARE
/// yielded, a leading/trailing separator yields a leading/trailing empty
/// field, and the empty string yields exactly one empty field — the field
/// count always equals separator-count + 1. Iterate with a `var` binding
/// (`next` is a pointer-receiver method).
pub const Splitter = struct {
    src: []u8,
    sep: u8,
    pos: usize,
    cur_off: usize,
    cur_len: usize,
    done: bool,

    /// Advance to the next field; `true` if one was produced.
    fn next(self: *Self) bool {
        if (self.done) {
            return false;
        }
        var start: usize = self.pos;
        var i: usize = start;
        while (i < self.src.len) : (i += 1) {
            if (self.src[i] == self.sep) {
                break;
            }
        }
        self.cur_off = start;
        self.cur_len = i - start;
        if (i < self.src.len) {
            self.pos = i + 1;
        } else {
            self.done = true;
        }
        return true;
    }

    /// The current field — call only after a `next()` that returned `true`.
    fn current(self: Self) []u8 {
        return self.src[self.cur_off..self.cur_off + self.cur_len];
    }
};

/// Iterator over the fields of a string split on a multi-byte separator
/// string. Same `next`/`current` protocol and empty-field rules as
/// `Splitter`; separator matches are found left-to-right and never overlap
/// (after a match the scan resumes past it, so `"aaa"` split on `"aa"` is
/// `""`, `"a"`). An EMPTY separator never matches: the whole source is
/// yielded as a single field.
pub const StrSplitter = struct {
    src: []u8,
    sep: []u8,
    pos: usize,
    cur_off: usize,
    cur_len: usize,
    done: bool,

    /// Advance to the next field; `true` if one was produced.
    fn next(self: *Self) bool {
        if (self.done) {
            return false;
        }
        self.cur_off = self.pos;
        if (self.sep.len == 0) {
            self.cur_len = self.src.len - self.pos;
            self.done = true;
            return true;
        }
        var m: i64 = strops_find_from(self.src, self.sep, self.pos);
        if (m < 0) {
            self.cur_len = self.src.len - self.pos;
            self.done = true;
        } else {
            self.cur_len = @as(usize, m) - self.pos;
            self.pos = @as(usize, m) + self.sep.len;
        }
        return true;
    }

    /// The current field — call only after a `next()` that returned `true`.
    fn current(self: Self) []u8 {
        return self.src[self.cur_off..self.cur_off + self.cur_len];
    }
};

/// A `Splitter` over the fields of `s` separated by the byte `sep` —
/// zero-copy, nothing to free. See `Splitter` for the iteration protocol
/// and the empty-field rules.
pub fn split_init(s: []u8, sep: u8) Splitter {
    return Splitter{
        .src = s,
        .sep = sep,
        .pos = 0,
        .cur_off = 0,
        .cur_len = 0,
        .done = false,
    };
}

/// A `StrSplitter` over the fields of `s` separated by the string `sep` —
/// zero-copy, nothing to free. See `StrSplitter` for the protocol, the
/// non-overlapping match rule and the empty-separator guard.
pub fn split_init_str(s: []u8, sep: []u8) StrSplitter {
    return StrSplitter{
        .src = s,
        .sep = sep,
        .pos = 0,
        .cur_off = 0,
        .cur_len = 0,
        .done = false,
    };
}

/// All fields of `s` split on the byte `sep`, in order, as `(off, len)`
/// spans into `s` (empty fields included — same rules as `Splitter`, so the
/// list always holds separator-count + 1 spans and is never empty). Returns
/// a fresh `ArrayList(SpanPair)`; release it with `.deinit(a)`.
pub fn split_collect(a: Allocator, s: []u8, sep: u8) ArrayList(SpanPair) {
    var out: ArrayList(SpanPair) = ArrayList(SpanPair).init(a);
    var it: Splitter = split_init(s, sep);
    while (it.next()) {
        out.push(a, SpanPair{ .off = it.cur_off, .len = it.cur_len });
    }
    return out;
}

/// `s` without leading ASCII whitespace (space/tab/LF/CR) — a zero-copy
/// subslice of `s`, nothing to free.
pub fn trim_start(s: []u8) []u8 {
    var i: usize = 0;
    while (i < s.len and strops_is_space(s[i])) {
        i += 1;
    }
    return s[i..s.len];
}

/// `s` without trailing ASCII whitespace (space/tab/LF/CR) — a zero-copy
/// subslice of `s`, nothing to free.
pub fn trim_end(s: []u8) []u8 {
    var j: usize = s.len;
    while (j > 0 and strops_is_space(s[j - 1])) {
        j -= 1;
    }
    return s[0..j];
}

/// `s` without leading or trailing ASCII whitespace (space/tab/LF/CR) —
/// zero-copy. Interior whitespace is kept; an all-whitespace `s` trims to
/// the empty string.
pub fn trim(s: []u8) []u8 {
    return trim_end(trim_start(s));
}

/// Join the `parts` spans of `src` with `sep` between consecutive fields:
/// `f0 sep f1 sep … fN`. An empty list joins to the empty string. Returns a
/// freshly-allocated string; free with `free(a, result)`. Round-trip law:
/// `join(a, s, split_collect(a, s, b), <the 1-byte string of b>)` rebuilds
/// `s` exactly.
pub fn join(a: Allocator, src: []u8, parts: ArrayList(SpanPair), sep: []u8) []u8 {
    var b: StrBuilder = StrBuilder.init(a);
    var k: usize = 0;
    while (k < parts.len()) : (k += 1) {
        if (k > 0) {
            b.append(a, sep);
        }
        var p: SpanPair = parts.get(k);
        b.append(a, src[p.off..p.off + p.len]);
    }
    var out: []u8 = b.build(a);
    b.deinit(a);
    return out;
}

/// `s` with every occurrence of `from` replaced by `to` — matches are found
/// left-to-right and never overlap (after a match the scan resumes past the
/// replaced bytes, so `replace("aaa", "aa", "b")` is `"ba"`). Returns a
/// freshly-allocated string (even when nothing matched); free with
/// `free(a, result)`. Guard: an EMPTY `from` matches nowhere and returns an
/// unchanged copy of `s`.
pub fn replace(a: Allocator, s: []u8, from: []u8, to: []u8) []u8 {
    var b: StrBuilder = StrBuilder.init(a);
    if (from.len == 0) {
        b.append(a, s);
    } else {
        var i: usize = 0;
        while (i < s.len) {
            if (strops_match_at(s, from, i)) {
                b.append(a, to);
                i += from.len;
            } else {
                b.append_byte(a, s[i]);
                i += 1;
            }
        }
    }
    var out: []u8 = b.build(a);
    b.deinit(a);
    return out;
}

// --- glob — glob pattern matching over []u8 ----------------------------------
//
// A single entry point, `glob_match`, implementing the classic shell-style
// glob dialect over raw bytes with an ITERATIVE two-pointer star-backtrack
// matcher (no recursion), plus `glob_is_literal` to detect pattern-free
// strings. Worst case O(len(pattern) * len(text)); typical patterns are
// linear.

// Length in bytes of the single pattern element starting at `p` (`p` must be
// in range). `\x` escapes span 2 bytes (a dangling trailing `\` spans 1);
// `[...]` classes span up to and including their closing `]`, where a `]`
// directly after `[` or `[!` counts as a literal member, not the closer; an
// unterminated class falls back to a 1-byte literal `[`. Everything else
// (including `*` and `?`) is 1 byte.
fn glob_elem_len(pattern: []u8, p: usize) usize {
    var c: u8 = pattern[p];
    if (c == 92) {                                     // '\\' escape
        if (p + 1 < pattern.len) {
            return 2;
        }
        return 1;                                      // dangling: literal '\\'
    }
    if (c == 91) {                                     // '[' class
        var i: usize = p + 1;
        if (i < pattern.len and pattern[i] == 33) {    // '!' negation
            i += 1;
        }
        if (i < pattern.len and pattern[i] == 93) {    // ']' first => literal member
            i += 1;
        }
        while (i < pattern.len) : (i += 1) {
            if (pattern[i] == 93) {                    // closing ']'
                return (i + 1) - p;
            }
        }
        return 1;                                      // unterminated: literal '['
    }
    return 1;                                          // '?', '*' or plain byte
}

// Does the single pattern element starting at `p` match the text byte `c`?
// Handles `?`, `\x` escapes, `[...]` / `[!...]` classes with inclusive byte
// ranges, and plain literal bytes. `*` is never passed here (the main loop
// consumes stars itself); were it passed, it would compare as a literal.
fn glob_elem_match(pattern: []u8, p: usize, c: u8) bool {
    var pc: u8 = pattern[p];
    if (pc == 63) {                                    // '?': any one byte
        return true;
    }
    if (pc == 92) {                                    // '\\' escape
        if (p + 1 < pattern.len) {
            return pattern[p + 1] == c;
        }
        return c == 92;                                // dangling: literal '\\'
    }
    if (pc == 91) {                                    // '[' class
        var elen: usize = glob_elem_len(pattern, p);
        if (elen == 1) {
            return c == 91;                            // unterminated: literal '['
        }
        var i: usize = p + 1;
        var neg: bool = false;
        if (pattern[i] == 33) {                        // '!'
            neg = true;
            i += 1;
        }
        var last: usize = p + elen - 1;                // index of the closing ']'
        var hit: bool = false;
        while (i < last) {
            if (i + 2 < last and pattern[i + 1] == 45) {   // 'x-y' range ('-')
                if (pattern[i] <= c and c <= pattern[i + 2]) {
                    hit = true;
                }
                i += 3;
            } else {
                if (pattern[i] == c) {                 // literal member
                    hit = true;
                }
                i += 1;
            }
        }
        if (neg) {
            return !hit;
        }
        return hit;
    }
    return pc == c;                                    // plain literal byte
}

/// Glob-match `pattern` against the whole of `text` (raw bytes, case
/// sensitive, no special treatment of `/` or leading dots). Supported
/// dialect:
///
///   - `*` matches any run of bytes, including the empty run.
///   - `?` matches exactly one byte of any value.
///   - `[abc]` matches one byte that is a member; `[a-z]` is an inclusive
///     byte-value range; members and ranges mix (`[a-cx0-9]`). `[!...]`
///     negates the set (`!` only — `^` is not a negation marker). A `]`
///     placed directly after `[` or `[!` is a literal member; a `-` that is
///     first, last, or right after a range is literal. A class needs a
///     closing `]`: an unterminated `[` (this includes `[]`, whose `]` is
///     taken as a member) matches a literal `[` byte. Backslash is NOT
///     special inside a class.
///   - `\x` matches the pattern byte `x` literally (`\*` matches `*`); a
///     trailing lone `\` matches a literal backslash.
///   - every other byte matches itself.
///
/// Iterative two-pointer matcher with star backtracking: on a mismatch after
/// a `*`, the most recent star re-expands by one byte and matching resumes —
/// for whole-string glob matching only the latest star ever needs to retry,
/// so no recursion or stack is required. Worst case O(len(pattern) *
/// len(text)).
pub fn glob_match(pattern: []u8, text: []u8) bool {
    var p: usize = 0;        // next pattern element
    var t: usize = 0;        // next text byte
    var has_star: bool = false;
    var star_p: usize = 0;   // pattern index just after the latest '*'
    var star_t: usize = 0;   // text index the latest '*' expansion started at
    while (t < text.len) {
        if (p < pattern.len and pattern[p] == 42) {    // '*'
            star_p = p + 1;
            star_t = t;
            has_star = true;
            p += 1;                                    // try the empty run first
        } else if (p < pattern.len and glob_elem_match(pattern, p, text[t])) {
            p += glob_elem_len(pattern, p);
            t += 1;
        } else if (has_star) {
            star_t += 1;                               // star eats one more byte
            t = star_t;
            p = star_p;
        } else {
            return false;
        }
    }
    // Text consumed: only (possibly star) pattern tail may remain.
    while (p < pattern.len and pattern[p] == 42) {
        p += 1;
    }
    return p == pattern.len;
}

/// True when `pattern` contains none of the glob metacharacter bytes `*`,
/// `?`, `[`, `\` — for such patterns `glob_match(pattern, text)` is exactly
/// `str_eq(pattern, text)`. Conservative: a pattern that only *behaves*
/// literally (e.g. an unterminated `[abc`) still returns `false`.
pub fn glob_is_literal(pattern: []u8) bool {
    for (pattern) |b| {
        if (b == 42 or b == 63 or b == 91 or b == 92) {
            return false;
        }
    }
    return true;
}

// --- json — a JSON parser + serializer (arena-style, zero-copy) -------------
//
// A real RFC-8259-shaped JSON reader/writer with no recursive types: the
// document is an ARENA of `JsonNode`s held in one growable `[]JsonNode`,
// linked by i32 indices (`first_child` / `next_sibling`, -1 = none). Strings,
// object keys and number texts are ZERO-COPY spans (`off`/`len`) into the
// original input, so a parsed `Json` borrows `src` and allocates only the
// node arena. Parse errors never panic: `Json.ok` goes false and `err_pos`
// records the byte offset of the FIRST error.

/// Node kind: the JSON `null` literal.
pub const JSON_NULL: u8 = 0;
/// Node kind: the JSON `false` literal.
pub const JSON_FALSE: u8 = 1;
/// Node kind: the JSON `true` literal.
pub const JSON_TRUE: u8 = 2;
/// Node kind: a number (`num` holds the f64 value, `str_off`/`str_len` its
/// source text).
pub const JSON_NUM: u8 = 3;
/// Node kind: a string (`str_off`/`str_len` span the RAW content between the
/// quotes — escapes validated but not decoded; see `Json.str_decode`).
pub const JSON_STR: u8 = 4;
/// Node kind: an array (children linked via `first_child`/`next_sibling`).
pub const JSON_ARR: u8 = 5;
/// Node kind: an object (children are the member values; each carries its
/// key's raw span in `key_off`/`key_len`).
pub const JSON_OBJ: u8 = 6;
/// The "no such node" kind returned by `Json.kind_of` for an invalid index.
pub const JSON_BAD: u8 = 255;

/// Maximum container nesting depth accepted by `json_parse` (a value nested
/// inside more than this many arrays/objects is a parse error, never a stack
/// overflow — the parser recurses at most this deep).
pub const JSON_MAX_DEPTH: i32 = 64;

// Is `c` an ASCII decimal digit?
fn json_is_digit(c: u8) bool {
    return c >= 48 and c <= 57;
}

// Is `c` an ASCII hex digit (0-9 a-f A-F)?
fn json_is_hex(c: u8) bool {
    if (c >= 48 and c <= 57) {
        return true;
    }
    if (c >= 97 and c <= 102) {
        return true;
    }
    return c >= 65 and c <= 70;
}

// m * 10^e in f64. `m` is a non-negative integer-valued mantissa accumulated
// from decimal digits. 10^k is formed by repeated multiplication (exact up to
// 10^22) and applied with ONE multiply or divide, so short decimals like
// "2.75" or "1e2" convert exactly; |e| is clamped to 400 (beyond that the
// result saturates to inf / 0 anyway). m == 0 short-circuits to 0 so a huge
// exponent on a zero mantissa cannot form 0 * inf (NaN).
fn json_scale10(m: f64, e: i64) f64 {
    if (m == 0.0) {
        return 0.0;
    }
    var k: i64 = e;
    if (k > 400) {
        k = 400;
    }
    if (k < 0 - 400) {
        k = 0 - 400;
    }
    var i: i64 = k;
    if (i < 0) {
        i = 0 - i;
    }
    var p: f64 = 1.0;
    while (i > 0) : (i -= 1) {
        p = p * 10.0;
    }
    if (k >= 0) {
        return m * p;
    }
    return m / p;
}

/// One arena node of a parsed JSON document. Nodes live in `Json.nodes` and
/// reference each other by index (i32, -1 = none) — JSON's recursive shape
/// with no recursive type. All spans index the ORIGINAL input (`Json.src`).
pub const JsonNode = struct {
    /// One of the `JSON_*` kind constants.
    kind: u8,
    /// The numeric value (kind `JSON_NUM` only; else 0).
    num: f64,
    /// Raw source span: string content between the quotes (escapes NOT
    /// decoded), or the number's text. 0/0 for other kinds.
    str_off: usize,
    str_len: usize,
    /// Raw source span of this node's object key (members of an object only;
    /// 0/0 elsewhere — an object member always has a key, possibly empty).
    key_off: usize,
    key_len: usize,
    /// Arena index of the first child (arrays/objects), -1 if none.
    first_child: i32,
    /// Arena index of the last child (parse-time O(1) append), -1 if none.
    last_child: i32,
    /// Arena index of the next sibling under the same parent, -1 if none.
    next_sibling: i32,
    /// Number of direct children (array elements / object members).
    child_count: i32,
};

/// A parsed JSON document: a node arena over a borrowed `src`. Build one with
/// `json_parse`, inspect it with the accessors below, serialize it back with
/// `json_emit`, release it with `deinit`. `src` must outlive the `Json` (all
/// string/key/number accessors return zero-copy views into it). On a parse
/// error `ok` is false, `err_pos` is the byte offset of the first error and
/// `root()` is -1; accessors on a failed parse return their miss values.
pub const Json = struct {
    /// The original input text (borrowed, never freed by `deinit`).
    src: []u8,
    /// The node arena; `count` entries of `nodes` are live.
    nodes: []JsonNode,
    count: usize,
    /// Arena index of the document's root value, -1 after a failed parse.
    root_idx: i32,
    /// True iff the whole input parsed as exactly one JSON value.
    ok: bool,
    /// Byte offset into `src` of the FIRST parse error (0 when `ok`).
    err_pos: usize,
    // Internal parse state: input cursor + the last scanned string span.
    pos: usize,
    scratch_off: usize,
    scratch_len: usize,

    // Record the first error position; later errors keep the first.
    fn set_err(self: *Self, p: usize) void {
        if (self.ok) {
            self.ok = false;
            self.err_pos = p;
        }
    }

    // Advance the cursor past JSON whitespace (space, tab, LF, CR).
    fn skip_ws(self: *Self) void {
        while (self.pos < self.src.len) {
            var c: u8 = self.src[self.pos];
            if (c == 32 or c == 9 or c == 10 or c == 13) {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    // Append a fresh zeroed node of `kind` to the arena (doubling growth)
    // and return its index.
    fn push_node(self: *Self, a: Allocator, kind: u8) i32 {
        if (self.count == self.nodes.len) {
            var grown: []JsonNode = alloc(a, JsonNode, self.nodes.len * 2);
            var i: usize = 0;
            while (i < self.count) : (i += 1) {
                grown[i] = self.nodes[i];
            }
            free(a, self.nodes);
            self.nodes = grown;
        }
        self.nodes[self.count] = JsonNode{
            .kind = kind,
            .num = 0.0,
            .str_off = 0,
            .str_len = 0,
            .key_off = 0,
            .key_len = 0,
            .first_child = 0 - 1,
            .last_child = 0 - 1,
            .next_sibling = 0 - 1,
            .child_count = 0,
        };
        self.count += 1;
        return @as(i32, self.count - 1);
    }

    // Link `child` as the last child of `parent` (O(1) via last_child).
    fn append_child(self: *Self, parent: i32, child: i32) void {
        var p: JsonNode = self.nodes[@as(usize, parent)];
        if (p.last_child >= 0) {
            self.nodes[@as(usize, p.last_child)].next_sibling = child;
        } else {
            p.first_child = child;
        }
        p.last_child = child;
        p.child_count += 1;
        self.nodes[@as(usize, parent)] = p;
    }

    // Scan a string at the cursor (src[pos] == '"'). Validates escapes
    // (\" \\ \/ \b \f \n \r \t and \uXXXX with 4 hex digits) and rejects
    // unescaped control bytes (< 0x20). On success the cursor is past the
    // closing quote and scratch_off/scratch_len span the RAW content.
    fn scan_string(self: *Self) bool {
        self.pos += 1;
        var start: usize = self.pos;
        while (self.pos < self.src.len) {
            var c: u8 = self.src[self.pos];
            if (c == 34) {
                self.scratch_off = start;
                self.scratch_len = self.pos - start;
                self.pos += 1;
                return true;
            }
            if (c == 92) {
                self.pos += 1;
                if (self.pos >= self.src.len) {
                    self.set_err(self.pos);
                    return false;
                }
                var e: u8 = self.src[self.pos];
                if (e == 34 or e == 92 or e == 47 or e == 98 or e == 102
                    or e == 110 or e == 114 or e == 116) {
                    // " \ / b f n r t
                    self.pos += 1;
                } else if (e == 117) {
                    // \uXXXX — exactly four hex digits
                    self.pos += 1;
                    var k: i32 = 0;
                    while (k < 4) : (k += 1) {
                        if (self.pos >= self.src.len) {
                            self.set_err(self.pos);
                            return false;
                        }
                        if (!json_is_hex(self.src[self.pos])) {
                            self.set_err(self.pos);
                            return false;
                        }
                        self.pos += 1;
                    }
                } else {
                    self.set_err(self.pos);
                    return false;
                }
            } else if (c < 32) {
                self.set_err(self.pos);
                return false;
            } else {
                self.pos += 1;
            }
        }
        self.set_err(self.pos);
        return false;
    }

    // Parse a number at the cursor (strict JSON grammar: optional '-', "0" or
    // [1-9][0-9]*, optional fraction, optional exponent — no leading zeros,
    // no bare '.', no '+' sign on the mantissa). The f64 value accumulates
    // decimal digits and scales once via json_scale10: short decimals are
    // exact; very long mantissas round in f64; huge exponents saturate to
    // inf / 0. Returns the node index, or -1 after set_err.
    fn parse_number(self: *Self, a: Allocator) i32 {
        var start: usize = self.pos;
        var neg: bool = false;
        if (self.src[self.pos] == 45) {
            neg = true;
            self.pos += 1;
        }
        if (self.pos >= self.src.len) {
            self.set_err(self.pos);
            return 0 - 1;
        }
        if (!json_is_digit(self.src[self.pos])) {
            self.set_err(self.pos);
            return 0 - 1;
        }
        var m: f64 = 0.0;
        if (self.src[self.pos] == 48) {
            // a leading 0 is a whole integer part by itself
            self.pos += 1;
        } else {
            while (self.pos < self.src.len and json_is_digit(self.src[self.pos])) {
                m = m * 10.0 + @as(f64, @as(i64, self.src[self.pos]) - 48);
                self.pos += 1;
            }
        }
        var fd: i64 = 0;
        if (self.pos < self.src.len and self.src[self.pos] == 46) {
            self.pos += 1;
            if (self.pos >= self.src.len) {
                self.set_err(self.pos);
                return 0 - 1;
            }
            if (!json_is_digit(self.src[self.pos])) {
                self.set_err(self.pos);
                return 0 - 1;
            }
            while (self.pos < self.src.len and json_is_digit(self.src[self.pos])) {
                m = m * 10.0 + @as(f64, @as(i64, self.src[self.pos]) - 48);
                fd += 1;
                self.pos += 1;
            }
        }
        var e10: i64 = 0;
        if (self.pos < self.src.len and (self.src[self.pos] == 101 or self.src[self.pos] == 69)) {
            self.pos += 1;
            var eneg: bool = false;
            if (self.pos < self.src.len and (self.src[self.pos] == 43 or self.src[self.pos] == 45)) {
                eneg = self.src[self.pos] == 45;
                self.pos += 1;
            }
            if (self.pos >= self.src.len) {
                self.set_err(self.pos);
                return 0 - 1;
            }
            if (!json_is_digit(self.src[self.pos])) {
                self.set_err(self.pos);
                return 0 - 1;
            }
            var ev: i64 = 0;
            while (self.pos < self.src.len and json_is_digit(self.src[self.pos])) {
                if (ev < 100000) {
                    // clamp: anything larger only saturates further
                    ev = ev * 10 + (@as(i64, self.src[self.pos]) - 48);
                }
                self.pos += 1;
            }
            if (eneg) {
                e10 = 0 - ev;
            } else {
                e10 = ev;
            }
        }
        var v: f64 = json_scale10(m, e10 - fd);
        if (neg) {
            v = 0.0 - v;
        }
        var id: i32 = self.push_node(a, JSON_NUM);
        var n: JsonNode = self.nodes[@as(usize, id)];
        n.num = v;
        n.str_off = start;
        n.str_len = self.pos - start;
        self.nodes[@as(usize, id)] = n;
        return id;
    }

    // Parse an array at the cursor (src[pos] == '['). Strict commas: no
    // trailing comma, elements separated by exactly one ','.
    fn parse_array(self: *Self, a: Allocator, depth: i32) i32 {
        var id: i32 = self.push_node(a, JSON_ARR);
        self.pos += 1;
        self.skip_ws();
        if (self.pos < self.src.len and self.src[self.pos] == 93) {
            self.pos += 1;
            return id;
        }
        while (true) {
            var child: i32 = self.parse_value(a, depth + 1);
            if (child < 0) {
                return 0 - 1;
            }
            self.append_child(id, child);
            self.skip_ws();
            if (self.pos >= self.src.len) {
                self.set_err(self.pos);
                return 0 - 1;
            }
            var c: u8 = self.src[self.pos];
            if (c == 93) {
                self.pos += 1;
                return id;
            }
            if (c != 44) {
                self.set_err(self.pos);
                return 0 - 1;
            }
            self.pos += 1;
        }
        return 0 - 1; // unreachable: the loop always returns
    }

    // Parse an object at the cursor (src[pos] == '{'). Each member is
    // "key" ':' value; the member VALUE node carries the key's raw span.
    fn parse_object(self: *Self, a: Allocator, depth: i32) i32 {
        var id: i32 = self.push_node(a, JSON_OBJ);
        self.pos += 1;
        self.skip_ws();
        if (self.pos < self.src.len and self.src[self.pos] == 125) {
            self.pos += 1;
            return id;
        }
        while (true) {
            self.skip_ws();
            if (self.pos >= self.src.len) {
                self.set_err(self.pos);
                return 0 - 1;
            }
            if (self.src[self.pos] != 34) {
                self.set_err(self.pos);
                return 0 - 1;
            }
            if (!self.scan_string()) {
                return 0 - 1;
            }
            var koff: usize = self.scratch_off;
            var klen: usize = self.scratch_len;
            self.skip_ws();
            if (self.pos >= self.src.len) {
                self.set_err(self.pos);
                return 0 - 1;
            }
            if (self.src[self.pos] != 58) {
                self.set_err(self.pos);
                return 0 - 1;
            }
            self.pos += 1;
            var child: i32 = self.parse_value(a, depth + 1);
            if (child < 0) {
                return 0 - 1;
            }
            var n: JsonNode = self.nodes[@as(usize, child)];
            n.key_off = koff;
            n.key_len = klen;
            self.nodes[@as(usize, child)] = n;
            self.append_child(id, child);
            self.skip_ws();
            if (self.pos >= self.src.len) {
                self.set_err(self.pos);
                return 0 - 1;
            }
            var c: u8 = self.src[self.pos];
            if (c == 125) {
                self.pos += 1;
                return id;
            }
            if (c != 44) {
                self.set_err(self.pos);
                return 0 - 1;
            }
            self.pos += 1;
        }
        return 0 - 1; // unreachable: the loop always returns
    }

    // Parse one JSON value at the cursor. Bounded recursion: `depth` counts
    // container nesting from 1 at the root; past JSON_MAX_DEPTH it errors.
    fn parse_value(self: *Self, a: Allocator, depth: i32) i32 {
        if (depth > JSON_MAX_DEPTH) {
            self.set_err(self.pos);
            return 0 - 1;
        }
        self.skip_ws();
        if (self.pos >= self.src.len) {
            self.set_err(self.pos);
            return 0 - 1;
        }
        var c: u8 = self.src[self.pos];
        if (c == 110) {
            // n
            if (str_starts_with(self.src[self.pos..self.src.len], "null")) {
                self.pos += 4;
                return self.push_node(a, JSON_NULL);
            }
            self.set_err(self.pos);
            return 0 - 1;
        }
        if (c == 116) {
            // t
            if (str_starts_with(self.src[self.pos..self.src.len], "true")) {
                self.pos += 4;
                return self.push_node(a, JSON_TRUE);
            }
            self.set_err(self.pos);
            return 0 - 1;
        }
        if (c == 102) {
            // f
            if (str_starts_with(self.src[self.pos..self.src.len], "false")) {
                self.pos += 5;
                return self.push_node(a, JSON_FALSE);
            }
            self.set_err(self.pos);
            return 0 - 1;
        }
        if (c == 34) {
            // "
            if (!self.scan_string()) {
                return 0 - 1;
            }
            var off: usize = self.scratch_off;
            var len: usize = self.scratch_len;
            var id: i32 = self.push_node(a, JSON_STR);
            var n: JsonNode = self.nodes[@as(usize, id)];
            n.str_off = off;
            n.str_len = len;
            self.nodes[@as(usize, id)] = n;
            return id;
        }
        if (c == 91) {
            // [
            return self.parse_array(a, depth);
        }
        if (c == 123) {
            // {
            return self.parse_object(a, depth);
        }
        if (c == 45 or json_is_digit(c)) {
            return self.parse_number(a);
        }
        self.set_err(self.pos);
        return 0 - 1;
    }

    /// Arena index of the root value, or -1 after a failed parse.
    fn root(self: Self) i32 {
        return self.root_idx;
    }

    /// The kind of node `idx` (a `JSON_*` constant), or `JSON_BAD` when `idx`
    /// is not a live node index.
    fn kind_of(self: Self, idx: i32) u8 {
        if (idx < 0 or @as(usize, idx) >= self.count) {
            return JSON_BAD;
        }
        return self.nodes[@as(usize, idx)].kind;
    }

    /// The numeric value of a `JSON_NUM` node; 0.0 for any other/invalid node.
    fn num_at(self: Self, idx: i32) f64 {
        if (self.kind_of(idx) != JSON_NUM) {
            return 0.0;
        }
        return self.nodes[@as(usize, idx)].num;
    }

    /// Zero-copy view of node `idx`'s raw source span: a string's content
    /// between the quotes (escape sequences still encoded — `\n` is the two
    /// bytes `\` `n`; use `str_decode` for the decoded bytes) or a number's
    /// exact source text. Empty for any other/invalid node. A VIEW into
    /// `src` — do not free it.
    fn str_at(self: Self, idx: i32) []u8 {
        var k: u8 = self.kind_of(idx);
        if (k != JSON_STR and k != JSON_NUM) {
            return "";
        }
        var n: JsonNode = self.nodes[@as(usize, idx)];
        var hi: usize = n.str_off + n.str_len;
        return self.src[n.str_off..hi];
    }

    /// Zero-copy view of the raw (still-escaped) object key carried by node
    /// `idx` (a member value of some object). Empty for non-members and
    /// invalid indexes. A VIEW into `src` — do not free it.
    fn key_at(self: Self, idx: i32) []u8 {
        if (idx < 0 or @as(usize, idx) >= self.count) {
            return "";
        }
        var n: JsonNode = self.nodes[@as(usize, idx)];
        var hi: usize = n.key_off + n.key_len;
        return self.src[n.key_off..hi];
    }

    /// Decode a `JSON_STR` node's content into a freshly-allocated `[]u8`
    /// (free it with `free(a, result)`): `\" \\ \/ \b \f \n \r \t` become
    /// their bytes; a `\uXXXX` escape decodes to a single `?` placeholder
    /// byte (documented limitation — no UTF-8 encoding of code points).
    /// Non-escape bytes (including UTF-8 sequences) copy through unchanged.
    /// Any other/invalid node yields a freshly-allocated empty slice.
    fn str_decode(self: Self, a: Allocator, idx: i32) []u8 {
        if (self.kind_of(idx) != JSON_STR) {
            return alloc(a, u8, 0);
        }
        var n: JsonNode = self.nodes[@as(usize, idx)];
        var b: StrBuilder = StrBuilder.init(a);
        var i: usize = 0;
        while (i < n.str_len) {
            var c: u8 = self.src[n.str_off + i];
            if (c == 92) {
                i += 1;
                var e: u8 = self.src[n.str_off + i];
                if (e == 98) {
                    b.append_byte(a, 8); // \b
                } else if (e == 102) {
                    b.append_byte(a, 12); // \f
                } else if (e == 110) {
                    b.append_byte(a, 10); // \n
                } else if (e == 114) {
                    b.append_byte(a, 13); // \r
                } else if (e == 116) {
                    b.append_byte(a, 9); // \t
                } else if (e == 117) {
                    b.append_byte(a, 63); // \uXXXX -> '?'
                    i += 4;
                } else {
                    b.append_byte(a, e); // \" \\ \/ keep the escaped byte
                }
                i += 1;
            } else {
                b.append_byte(a, c);
                i += 1;
            }
        }
        var out: []u8 = b.build(a);
        b.deinit(a);
        return out;
    }

    /// Number of direct children of an array (elements) or object (members);
    /// 0 for any other/invalid node.
    fn arr_len(self: Self, idx: i32) usize {
        var k: u8 = self.kind_of(idx);
        if (k != JSON_ARR and k != JSON_OBJ) {
            return 0;
        }
        return @as(usize, self.nodes[@as(usize, idx)].child_count);
    }

    /// Arena index of child `i` (0-based) of an array — or, positionally, of
    /// an object's i-th member value. -1 when out of range or `idx` is not a
    /// container. O(i) sibling walk.
    fn arr_get(self: Self, idx: i32, i: usize) i32 {
        var k: u8 = self.kind_of(idx);
        if (k != JSON_ARR and k != JSON_OBJ) {
            return 0 - 1;
        }
        var cur: i32 = self.nodes[@as(usize, idx)].first_child;
        var step: usize = 0;
        while (cur >= 0 and step < i) {
            cur = self.nodes[@as(usize, cur)].next_sibling;
            step += 1;
        }
        return cur;
    }

    /// Arena index of the member value whose key equals `key` in object
    /// `idx`, or -1 (missing key, or `idx` not an object). Keys compare as
    /// RAW bytes (`str_eq` on the undecoded span): a key containing escapes
    /// must be queried in its escaped spelling. With duplicate keys the
    /// FIRST match wins.
    fn obj_get(self: Self, idx: i32, key: []u8) i32 {
        if (self.kind_of(idx) != JSON_OBJ) {
            return 0 - 1;
        }
        var cur: i32 = self.nodes[@as(usize, idx)].first_child;
        while (cur >= 0) {
            var n: JsonNode = self.nodes[@as(usize, cur)];
            var hi: usize = n.key_off + n.key_len;
            if (str_eq(self.src[n.key_off..hi], key)) {
                return cur;
            }
            cur = n.next_sibling;
        }
        return 0 - 1;
    }

    /// Release the node arena (pass the allocator given to `json_parse`).
    /// `src` is borrowed and is NOT freed. The `Json` must not be used
    /// afterwards.
    fn deinit(self: Self, a: Allocator) void {
        free(a, self.nodes);
    }
};

/// Parse `src` as one JSON document into an arena `Json` (release it with
/// `deinit(a)`; `src` is borrowed and must outlive the result). Full JSON:
/// `null`/`true`/`false`, numbers (sign, fraction, exponent — value held as
/// `f64`, short decimals exactly; the original text is kept too), strings
/// with escape validation (`\" \\ \/ \b \f \n \r \t`, `\uXXXX`), arrays,
/// objects, and `space`/`tab`/`CR`/`LF` whitespace. Strict where JSON is:
/// no trailing commas, no leading zeros ("01" fails), no trailing garbage
/// after the root value, unescaped control bytes rejected, container depth
/// capped at `JSON_MAX_DEPTH`. On ANY error the result has `ok == false`,
/// `err_pos` = the first bad byte's offset, and `root() == -1` (so every
/// accessor returns its documented miss value).
pub fn json_parse(a: Allocator, src: []u8) Json {
    var j: Json = Json{
        .src = src,
        .nodes = alloc(a, JsonNode, 8),
        .count = 0,
        .root_idx = 0 - 1,
        .ok = true,
        .err_pos = 0,
        .pos = 0,
        .scratch_off = 0,
        .scratch_len = 0,
    };
    var root: i32 = j.parse_value(a, 1);
    if (root >= 0) {
        j.skip_ws();
        if (j.pos < j.src.len) {
            j.set_err(j.pos); // trailing garbage after the root value
        } else {
            j.root_idx = root;
        }
    }
    return j;
}

// Serialize the subtree rooted at `idx` into `b` (minified). Strings and
// numbers are reproduced from their raw source spans — already-valid JSON
// text, so emission is escape-correct and numbers lose no precision.
// Recursion is bounded by JSON_MAX_DEPTH (every Json comes from json_parse).
fn json_emit_node(a: Allocator, j: Json, b: *StrBuilder, idx: i32) void {
    var n: JsonNode = j.nodes[@as(usize, idx)];
    if (n.kind == JSON_NULL) {
        b.append(a, "null");
        return;
    }
    if (n.kind == JSON_FALSE) {
        b.append(a, "false");
        return;
    }
    if (n.kind == JSON_TRUE) {
        b.append(a, "true");
        return;
    }
    if (n.kind == JSON_NUM or n.kind == JSON_STR) {
        var hi: usize = n.str_off + n.str_len;
        if (n.kind == JSON_STR) {
            b.append_byte(a, 34); // "
        }
        b.append(a, j.src[n.str_off..hi]);
        if (n.kind == JSON_STR) {
            b.append_byte(a, 34); // "
        }
        return;
    }
    if (n.kind != JSON_ARR and n.kind != JSON_OBJ) {
        return; // defensive: unreachable for nodes built by json_parse
    }
    var open: u8 = 91; // [
    var close: u8 = 93; // ]
    if (n.kind == JSON_OBJ) {
        open = 123; // {
        close = 125; // }
    }
    b.append_byte(a, open);
    var cur: i32 = n.first_child;
    var first: bool = true;
    while (cur >= 0) {
        if (!first) {
            b.append_byte(a, 44); // ,
        }
        first = false;
        var c: JsonNode = j.nodes[@as(usize, cur)];
        if (n.kind == JSON_OBJ) {
            b.append_byte(a, 34); // "
            var khi: usize = c.key_off + c.key_len;
            b.append(a, j.src[c.key_off..khi]);
            b.append_byte(a, 34); // "
            b.append_byte(a, 58); // :
        }
        json_emit_node(a, j, b, cur);
        cur = c.next_sibling;
    }
    b.append_byte(a, close);
}

/// Serialize `j` back to JSON text in a freshly-allocated `[]u8` (free it
/// with `free(a, result)`). Output is MINIFIED (no whitespace). Strings,
/// object keys and numbers are emitted from their raw source spans, so the
/// text round-trips losslessly (escapes preserved verbatim, numbers keep
/// their exact original spelling — `0.50` stays `0.50`, `1e2` stays `1e2`).
/// A failed parse (`ok == false`) yields an empty (freshly-allocated) slice.
pub fn json_emit(a: Allocator, j: Json) []u8 {
    var b: StrBuilder = StrBuilder.init(a);
    if (j.root_idx >= 0) {
        json_emit_node(a, j, &b, j.root_idx);
    }
    var out: []u8 = b.build(a);
    b.deinit(a);
    return out;
}
