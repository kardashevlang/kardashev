//SPEC: §25.1 an anonymous `struct { … }` type value is only valid as a type-constructor body — any ordinary value position is E0310
//ERR: E0310

// `pick` returns `i64`, not `type`, so it is not a type-constructor; its
// `struct { … }` expression reaches the ordinary expression checker.
fn pick() i64 {
    return struct { a: i64 };
}

pub fn main() void {
    print(pick());
}
