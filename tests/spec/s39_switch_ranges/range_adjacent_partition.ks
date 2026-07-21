//SPEC: §39 adjacent inclusive ranges partition an integer domain without overlap
//OUT: 1
//OUT: 2
//OUT: 2
//OUT: 3

fn cls(x: i64) i64 {
    switch (x) {
        0..4 => { return 1; },
        5..9 => { return 2; },
        else => { return 3; },
    }
}

pub fn main() void {
    print(cls(4));
    print(cls(5));
    print(cls(9));
    print(cls(10));
}
