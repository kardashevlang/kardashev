//SPEC: §42.2 applying a name that is not a type-constructor in TYPE position is E0312
//ERR: E0312

// `plain` exists but is an ordinary function, not a type-constructor.
fn plain(x: i32) i32 {
    return x;
}

pub fn main() void {
    var b: plain(i32) = plain(1);
    print(1);
}
