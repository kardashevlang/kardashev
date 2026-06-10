//SPEC: §21.1 `if (opt) |v|` binds the unwrapped T in then; the else arm runs when null
//OUT: 8
//OUT: -1

// `first_even` derives its optional through a real scan, so which path the
// `if` takes depends on the data, not on a literal.
fn first_even(xs: []i64) ?i64 {
    var i: usize = 0;
    while (i < xs.len) : (i = i + 1) {
        if (xs[i] % 2 == 0) {
            return xs[i];
        }
    }
    return null;
}

pub fn main() void {
    var data: [5]i64 = [5]i64{ 1, 3, 8, 9, 12 };

    if (first_even(data[0..5])) |e| {
        print(e);               // 8 — the unwrapped i64
    } else {
        print(0 - 1);
    }

    if (first_even(data[0..2])) |e| {   // {1,3}: no even -> null
        print(e);
    } else {
        print(0 - 1);           // -1 — the else arm on null
    }
}
