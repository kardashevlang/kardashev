//SPEC: §31.1 ALL of a type-constructor's comptime parameters must be `type` — a comptime VALUE parameter in a type-constructor is rejected
//ERR: E0310
fn Bad(comptime T: type, comptime n: usize) type {
    return struct {
        x: T,
    };
}

const A = Bad(i64, 3);

pub fn main() void {
    print(1);
}
