//SPEC: §39.1 overlapping integer `switch` ranges/labels are a sema error (E0211), not a raw cc failure
//ERR: E0211

fn f(n: i64) i64 {
    switch (n) {
        1..5 => { return 1; },
        3..8 => { return 2; },   // overlaps [3,5] — cc: duplicate case value
        else => { return 0; },
    }
}

pub fn main() void {
    print(f(4));
    print(f(7));
}
