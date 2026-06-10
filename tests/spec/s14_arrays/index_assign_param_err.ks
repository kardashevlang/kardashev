//SPEC: §14.2 index-assignment needs a mutable `var` array — a by-value parameter is not assignable
//ERR: E0223

fn zap(a: [2]i64) i64 {
    a[0] = 9;          // parameters are immutable bindings
    return a[0];
}

pub fn main() void {
    print(zap([2]i64{ 1, 2 }));
}
