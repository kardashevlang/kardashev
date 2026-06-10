//SPEC: §42.1 `[N]Name(A)` — a fixed-size array of an application: literal, indexing, element method call
//OUT: 42
//OUT: 17

fn Box(comptime T: type) type {
    return struct {
        v: T,
        fn init(x: T) Self {
            return Self{ .v = x };
        }
        fn get(self: Self) T {
            return self.v;
        }
    };
}

pub fn main() void {
    // The array TYPE and the element constructors are all direct applications.
    var arr: [3]Box(i64) = [3]Box(i64){
        Box(i64).init(40),
        Box(i64).init(2),
        Box(i64).init(9),
    };
    print(arr[0].get() + arr[1].get());   // method on an indexed array element
    arr[2] = Box(i64).init(17);           // element assignment
    print(arr[2].get());
}
