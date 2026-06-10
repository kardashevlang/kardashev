//SPEC: §17.1 the composite form `[]T` substitutes through generic params and returns
//OUT: 21
//OUT: 3
//OUT: pec

// `[]T` must substitute in both parameter and return position. Instantiated at
// i32 (numbers) and u8 (a string literal is a []u8, §23) — if substitution
// produced the wrong slice type either the sum or the printed text would break.
fn tail(comptime T: type, xs: []T) []T {
    return xs[1..xs.len];
}

fn total(comptime T: type, xs: []T) T {
    var s: T = 0;
    var i: usize = 0;
    while (i < xs.len) : (i = i + 1) {
        s = s + xs[i];
    }
    return s;
}

pub fn main() void {
    var a: [4]i32 = [4]i32{ 5, 6, 7, 8 };
    var t: []i32 = tail(i32, a[0..4]);
    print(total(i32, t));       // 6 + 7 + 8 = 21
    print(t.len);               // 3
    print(tail(u8, "spec"));    // drops 's' -> "pec"
}
