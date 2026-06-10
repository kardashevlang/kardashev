//SPEC: §42.2 an application and an alias of the same (ctor, args) share ONE struct — values interchange freely
//OUT: 6
//OUT: 60
//OUT: 61

// If `Box(i32)` and `const B = Box(i32);` produced two distinct structs,
// every call below would be a type mismatch. They must be the same instance.
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

const B = Box(i32);

fn via_app(b: Box(i32)) i32 {
    return b.get();
}

fn via_alias(b: B) i32 {
    return b.get();
}

fn bump_app(p: *Box(i32)) void {
    p.v = p.v + 1;
}

pub fn main() void {
    var from_alias: B = B.init(6);
    var from_app: Box(i32) = Box(i32).init(60);
    print(via_app(from_alias));        // alias value into app-typed param: 6
    print(via_alias(from_app));        // app value into alias-typed param: 60
    var mixed: B = from_app;           // alias-typed var holds the app value
    bump_app(&mixed);                  // &alias-typed var into a *Box(i32)
    print(mixed.get());                // 60 + 1 = 61
}
