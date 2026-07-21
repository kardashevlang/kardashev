//SPEC: §36 x §29 `catch |e|` inside a for maps each element's error code into a fallback
//OUT: 1
//OUT: 2
//OUT: 300

fn tri(v: i64) !i64 {
    if (v == 1) { return error.One; }
    if (v == 2) { return error.Two; }
    return v * 100;
}

pub fn main() void {
    var xs: [3]i64 = [3]i64{ 1, 2, 3 };
    for (xs) |v| {
        print(tri(v) catch |e| @as(i64, e));
    }
}
