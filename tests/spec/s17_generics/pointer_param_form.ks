//SPEC: §17.1 the composite form `*T` substitutes through a generic param (writes through the pointer)
//OUT: 10
//OUT: 1024

// `*T` must substitute to a concrete pointer type; the generic mutates the
// callee's variable through it, so the observable values exist only if the
// deref-assign really targeted the caller's storage.
fn add_into(comptime T: type, p: *T, v: T) void {
    p.* = p.* + v;
}

fn double_into(comptime T: type, p: *T) void {
    p.* = p.* * 2;
}

pub fn main() void {
    var x: i32 = 0;
    var i: i32 = 1;
    while (i < 5) : (i = i + 1) {
        add_into(i32, &x, i);       // 1 + 2 + 3 + 4
    }
    print(x);                       // 10

    var y: i64 = 1;
    var k: i64 = 0;
    while (k < 10) : (k = k + 1) {
        double_into(i64, &y);       // 2^10
    }
    print(y);                       // 1024
}
