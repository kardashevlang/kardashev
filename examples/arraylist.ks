// arraylist.ks — a generic growable list `ArrayList(T)` (v0.130).
//
// The std prelude's first container, built entirely in the language: a
// type-constructor returns a `struct` with METHODS (v0.130). The list owns a
// backing slice from the `Allocator` and grows by allocating a larger buffer,
// copying, and freeing the old one (there is no `realloc`). Methods take `self`
// by value and return a new `Self`, so usage is `list = list.append(a, x);`.

fn ArrayList(comptime T: type) type {
    return struct {
        items: []T,      // backing buffer; capacity == items.len
        count: usize,    // number of elements in use

        fn init(a: Allocator) Self {
            return Self{ .items = alloc(a, T, 4), .count = 0 };
        }

        fn append(self: Self, a: Allocator, x: T) Self {
            if (self.count < self.items.len) {
                var here: []T = self.items;
                here[self.count] = x;
                return Self{ .items = here, .count = self.count + 1 };
            }
            // Full: grow to twice the capacity, copy, free the old buffer.
            var grown: []T = alloc(a, T, self.items.len * 2);
            var i: usize = 0;
            while (i < self.count) : (i = i + 1) {
                grown[i] = self.items[i];
            }
            free(a, self.items);
            grown[self.count] = x;
            return Self{ .items = grown, .count = self.count + 1 };
        }

        fn get(self: Self, i: usize) T {
            return self.items[i];
        }

        fn len(self: Self) usize {
            return self.count;
        }

        fn deinit(self: Self, a: Allocator) void {
            free(a, self.items);
        }
    };
}

const IntList = ArrayList(i32);

fn sum(list: IntList) i32 {
    var total: i32 = 0;
    var i: usize = 0;
    while (i < list.len()) : (i = i + 1) {
        total = total + list.get(i);
    }
    return total;
}

pub fn main() i32 {
    var a: Allocator = c_allocator();
    var list: IntList = IntList.init(a);

    var i: i32 = 0;
    while (i < 8) : (i = i + 1) {
        list = list.append(a, i * i);   // 0 1 4 9 16 25 36 49 — forces a grow past cap 4
    }

    print(list.len());     // 8
    print(list.get(0));    // 0
    print(list.get(7));    // 49
    print(sum(list));      // 140
    list.deinit(a);
    return 0;
}

test "arraylist" {
    var a: Allocator = c_allocator();
    var list: IntList = IntList.init(a);
    var i: i32 = 0;
    while (i < 5) : (i = i + 1) {
        list = list.append(a, i + 1);   // 1 2 3 4 5
    }
    expect(list.len() == 5);
    expect(list.get(0) == 1);
    expect(list.get(4) == 5);
    expect(sum(list) == 15);
    list.deinit(a);
}
