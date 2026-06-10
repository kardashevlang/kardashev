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
