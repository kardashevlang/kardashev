//SPEC: §32.1×§26 `@sizeOf(Self)` / `@sizeOf(T)` inside a generic-struct method resolve per instantiation — three monomorphs report three sizes
//OUT: 2
//OUT: 1
//OUT: 8
//OUT: 4
//OUT: 16
//OUT: 8

// Both fields share T, so sizeof(Pair(T)) = 2 * sizeof(T) with no padding;
// the fixed-width ints make the values exact (u8=1, i32=4, i64=8).
fn Pair(comptime T: type) type {
    return struct {
        a: T,
        b: T,
        fn size(self: Self) usize {
            return @sizeOf(Self);
        }
        fn elem(self: Self) usize {
            return @sizeOf(T);
        }
    };
}

const P8 = Pair(u8);
const P32 = Pair(i32);
const P64 = Pair(i64);

pub fn main() void {
    var x: P8 = P8{ .a = 1, .b = 2 };
    var y: P32 = P32{ .a = 1, .b = 2 };
    var z: P64 = P64{ .a = 1, .b = 2 };
    print(x.size());
    print(x.elem());
    print(y.size());
    print(y.elem());
    print(z.size());
    print(z.elem());
}
