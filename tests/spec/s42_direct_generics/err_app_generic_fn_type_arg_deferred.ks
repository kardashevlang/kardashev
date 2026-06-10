//SPEC: §42.4 an application as a generic FUNCTION's type argument stays deferred — E0251 (type args are bare names)
//ERR: E0251

fn Box(comptime T: type) type {
    return struct {
        v: T,
        fn init(x: T) Self {
            return Self{ .v = x };
        }
    };
}

fn id(comptime T: type, x: T) T {
    return x;
}

pub fn main() void {
    var b: Box(i32) = Box(i32).init(7);
    var c: Box(i32) = id(Box(i32), b);   // generic-fn type args stay bare names (§17)
    print(c.v);
}
