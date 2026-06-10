//SPEC: §26.2 a type-constructor's methods are monomorphised at alias instantiation and resolve like plain struct methods; `T` is in scope in their signatures and bodies
//OUT: 35
//OUT: 24

fn Acc(comptime T: type) type {
    return struct {
        hi: T,
        lo: T,

        fn spread(self: Self) T {
            return self.hi - self.lo;
        }

        fn mid(self: Self, bias: T) T {
            return (self.hi + self.lo) / 2 + bias;
        }
    };
}

const A = Acc(i64);

pub fn main() void {
    var a: A = A{ .hi = 40, .lo = 5 };
    print(a.spread()); // 40 - 5            = 35
    print(a.mid(2)); // 45/2 = 22, + 2    = 24
}
