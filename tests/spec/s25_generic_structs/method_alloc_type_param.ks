//SPEC: §26.2 `alloc(a, T, n)`'s type argument resolves through the method's substitution — methods manage heap buffers of the bound element type
//OUT: 47

fn Buf(comptime T: type) type {
    return struct {
        items: []T,

        fn init(a: Allocator, n: usize) Self {
            return Self{ .items = alloc(a, T, n) }; // T → i64 inside this instance
        }

        fn deinit(self: Self, a: Allocator) void {
            free(a, self.items);
        }
    };
}

const B = Buf(i64);

pub fn main() void {
    var a: Allocator = c_allocator();
    var b: B = B.init(a, 5);
    b.items[4] = 42;
    print(b.items[4] + @as(i64, b.items.len)); // 42 + 5 = 47
    b.deinit(a);
}
