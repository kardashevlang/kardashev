//SPEC: §21.1 an `if` capture `|v|` whose condition is not an optional (`?T`) is E0280
//ERR: E0280

pub fn main() void {
    var n: i64 = 3;
    if (n) |v| {       // n is a plain i64, not a ?i64
        print(v);
    } else {
        print(0);
    }
}
