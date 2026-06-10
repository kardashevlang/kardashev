//SPEC: §29 a `for` without `, 0..` takes exactly ONE capture `|elem|` — two captures are rejected
//ERR: E0200

pub fn main() void {
    var xs: [2]i64 = [2]i64{ 1, 2 };
    for (xs) |v, i| {
        print(v);
    }
}
