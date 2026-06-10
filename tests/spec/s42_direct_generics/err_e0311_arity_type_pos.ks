//SPEC: §42.2 an application with the wrong number of type arguments in type position is E0311
//ERR: E0311

fn Box(comptime T: type) type {
    return struct {
        v: T,
        fn init(x: T) Self {
            return Self{ .v = x };
        }
    };
}

pub fn main() void {
    var b: Box(i32, i64) = Box(i32).init(1);   // Box takes 1 type argument
    print(1);
}
