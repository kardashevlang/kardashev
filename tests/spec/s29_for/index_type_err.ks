//SPEC: §29.1 the index capture is `usize`, not i64 — initializing an `i64` from it is a type mismatch
//ERR: E0110

pub fn main() void {
    var xs: [2]i64 = [2]i64{ 1, 2 };
    for (xs, 0..) |v, i| {
        var t: i64 = i;
        print(t + v);
    }
}
