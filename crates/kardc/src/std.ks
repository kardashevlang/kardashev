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
