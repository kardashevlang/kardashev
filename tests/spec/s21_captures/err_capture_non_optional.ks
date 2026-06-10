//SPEC: §21.1 an `if` capture whose condition is not an optional `?T` is E0280
//ERR: E0280

pub fn main() void {
    var b: bool = 1 < 2;
    // A bool condition is fine for a plain `if`, but `|v|` demands a `?T`.
    if (b) |v| {
        print(v);
    } else {
        print(0);
    }
}
