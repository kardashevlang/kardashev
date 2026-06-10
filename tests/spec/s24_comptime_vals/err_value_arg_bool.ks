//SPEC: §24.2 a comptime value argument that const-folds to a bool is E0253 — an integer is required
//ERR: E0253

fn scale(comptime n: i64, x: i64) i64 {
    return n * x;
}

pub fn main() void {
    print(scale(2 > 1, 4)); // folds to `true`, not an integer
}
