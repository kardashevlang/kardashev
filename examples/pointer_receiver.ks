// pointer_receiver.ks — pointer-receiver methods & true mutation (v0.134).
//
// A method whose `self` is a pointer (`self: *Point` / `self: *Self`) mutates
// the receiver in place. The call site auto-refs (`p.move(...)` passes `&p`),
// and field access auto-derefs (`self.x` reads/writes through the pointer).
// A value receiver (`self: Point`) still takes a copy.

const Point = struct {
    x: i32,
    y: i32,
    // Pointer receivers: these mutate the caller's Point.
    fn move_by(self: *Point, dx: i32, dy: i32) void {
        self.x += dx;
        self.y += dy;
    }
    fn scale(self: *Point, k: i32) void {
        self.x *= k;
        self.y *= k;
    }
    // Value receiver: reads only, takes a copy.
    fn manhattan(self: Point) i32 {
        var ax: i32 = self.x;
        if (ax < 0) { ax = 0 - ax; }
        var ay: i32 = self.y;
        if (ay < 0) { ay = 0 - ay; }
        return ax + ay;
    }
};

// A growable stack — pointer receivers give it a real mutating API.
fn Stack(comptime T: type) type {
    return struct {
        buf: []T,
        n: usize,
        fn init(a: Allocator) Self { return Self{ .buf = alloc(a, T, 2), .n = 0 }; }
        fn push(self: *Self, a: Allocator, v: T) void {
            if (self.n == self.buf.len) {
                var bigger: []T = alloc(a, T, self.buf.len * 2);
                var i: usize = 0;
                while (i < self.n) : (i += 1) { bigger[i] = self.buf[i]; }
                free(a, self.buf);
                self.buf = bigger;
            }
            self.buf[self.n] = v;
            self.n += 1;
        }
        fn pop(self: *Self) T {
            self.n -= 1;
            return self.buf[self.n];
        }
        fn len(self: Self) usize { return self.n; }
        fn deinit(self: Self, a: Allocator) void { free(a, self.buf); }
    };
}

const IntStack = Stack(i32);

pub fn main() i32 {
    var p: Point = Point{ .x = 3, .y = 4 };
    p.move_by(1, 1);          // -> (4, 5)
    p.scale(2);               // -> (8, 10)
    print(p.x);               // 8
    print(p.y);               // 10
    print(p.manhattan());     // 18 (value receiver, no mutation)

    var a: Allocator = c_allocator();
    var s: IntStack = IntStack.init(a);
    var i: i32 = 1;
    while (i <= 5) : (i += 1) {
        s.push(a, i * 10);    // mutating push: 10 20 30 40 50 (forces a grow)
    }
    print(s.len());           // 5
    print(s.pop());           // 50
    print(s.pop());           // 40
    print(s.len());           // 3
    s.deinit(a);
    return 0;
}

test "pointer receivers" {
    var p: Point = Point{ .x = 0, .y = 0 };
    p.move_by(5, 0 - 3);
    expect(p.x == 5);
    expect(p.y == 0 - 3);
    expect(p.manhattan() == 8);

    var a: Allocator = c_allocator();
    var s: IntStack = IntStack.init(a);
    s.push(a, 7);
    s.push(a, 9);
    expect(s.len() == 2);
    expect(s.pop() == 9);
    s.deinit(a);
}
