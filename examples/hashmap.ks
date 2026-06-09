// hashmap.ks — a generic open-addressing hash map (v0.138).
//
// The second std container, written entirely in the language on top of the
// Arc-3 machinery: a type-constructor with multiple-... well, one value type
// parameter (v0.129/v0.135), pointer-receiver methods for mutation (v0.134),
// integer casts to hash an `i32` key into a `usize` slot (v0.137), and the
// `Allocator` (v0.119). Keys are `i32`; the value type `V` is generic.
//
// Linear probing with tombstones for `remove`, and a grow-and-rehash at a 0.75
// load factor.

const EMPTY: i32 = 0;
const FULL: i32 = 1;
const TOMB: i32 = 2;

fn HashMap(comptime V: type) type {
    return struct {
        keys: []i32,
        vals: []V,
        state: []i32,    // EMPTY / FULL / TOMB per slot
        cap: usize,
        count: usize,    // live entries (FULL only)

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
                s.state[i] = EMPTY;
            }
            return s;
        }

        fn init(a: Allocator) Self {
            return Self.with_cap(a, 8);
        }

        fn hash(self: Self, key: i32) usize {
            var k: i32 = key;
            if (k < 0) {
                k = 0 - k;
            }
            return @as(usize, k) % self.cap;
        }

        // The slot to read `key` from: the matching FULL slot, or the first
        // EMPTY slot if absent (tombstones are skipped, never matched).
        fn find(self: Self, key: i32) usize {
            var i: usize = self.hash(key);
            while (self.state[i] != EMPTY) : (i = (i + 1) % self.cap) {
                if (self.state[i] == FULL) {
                    if (self.keys[i] == key) {
                        return i;
                    }
                }
            }
            return i;
        }

        fn grow(self: *Self, a: Allocator) void {
            var old_keys: []i32 = self.keys;
            var old_vals: []V = self.vals;
            var old_state: []i32 = self.state;
            var old_cap: usize = self.cap;

            var fresh: Self = Self.with_cap(a, old_cap * 2);
            var i: usize = 0;
            while (i < old_cap) : (i += 1) {
                if (old_state[i] == FULL) {
                    fresh.insert(old_keys[i], old_vals[i]);
                }
            }
            free(a, old_keys);
            free(a, old_vals);
            free(a, old_state);
            self.keys = fresh.keys;
            self.vals = fresh.vals;
            self.state = fresh.state;
            self.cap = fresh.cap;
            self.count = fresh.count;
        }

        // Insert assuming there is room (used during grow and after a resize).
        fn insert(self: *Self, key: i32, val: V) void {
            var i: usize = self.find(key);
            if (self.state[i] != FULL) {
                self.state[i] = FULL;
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
            var i: usize = self.find(key);
            return self.state[i] == FULL;
        }

        // The value for `key`, or `fallback` if absent.
        fn get(self: Self, key: i32, fallback: V) V {
            var i: usize = self.find(key);
            if (self.state[i] == FULL) {
                return self.vals[i];
            }
            return fallback;
        }

        fn remove(self: *Self, key: i32) bool {
            var i: usize = self.find(key);
            if (self.state[i] == FULL) {
                self.state[i] = TOMB;
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

const Counts = HashMap(i32);

pub fn main() i32 {
    var a: Allocator = c_allocator();
    var m: Counts = Counts.init(a);

    // Insert 0..50 -> i*i; forces several grow-and-rehash cycles.
    var i: i32 = 0;
    while (i < 50) : (i += 1) {
        m.put(a, i, i * i);
    }
    print(m.len());            // 50
    print(m.get(7, 0 - 1));    // 49
    print(m.get(49, 0 - 1));   // 2401
    print(m.get(999, 0 - 1));  // -1 (absent)

    // Overwrite, remove, and re-probe across a tombstone.
    m.put(a, 7, 700);
    print(m.get(7, 0 - 1));    // 700
    print(m.len());            // 50 (overwrite, not insert)

    var removed: bool = m.remove(7);
    if (removed) { print(1); } else { print(0); }   // 1
    print(m.len());            // 49
    if (m.has(7)) { print(1); } else { print(0); }  // 0

    // Re-insert after removal (reuses a tombstone slot).
    m.put(a, 7, 77);
    print(m.get(7, 0 - 1));    // 77
    print(m.len());            // 50
    m.deinit(a);
    return 0;
}

test "hashmap" {
    var a: Allocator = c_allocator();
    var m: Counts = Counts.init(a);
    var i: i32 = 0;
    while (i < 30) : (i += 1) {
        m.put(a, i * 3, i);
    }
    expect(m.len() == 30);
    expect(m.get(0, 0 - 1) == 0);
    expect(m.get(87, 0 - 1) == 29);     // 29*3 == 87
    expect(m.get(1, 0 - 1) == 0 - 1);   // absent
    expect(m.has(45));                  // 15*3
    expect(m.remove(45));
    expect(!m.has(45));
    expect(m.len() == 29);
    m.put(a, 45, 123);                  // reuse tombstone
    expect(m.get(45, 0) == 123);
    expect(m.len() == 30);
    m.deinit(a);
}
