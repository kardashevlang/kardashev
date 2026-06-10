//SPEC: §39 an integer arm may list several value labels `1, 2, 3 =>` — the arm matches any of them
//OUT: 100
//OUT: 100
//OUT: 100
//OUT: 200
//OUT: 200

fn pick(n: i64) i64 {
    switch (n) {
        1, 2, 3 => { return 100; },
        4, 6 => { return 200; },
        else => { return 0 - 1; },
    }
}

pub fn main() void {
    print(pick(1));
    print(pick(2));
    print(pick(3));
    print(pick(4));
    print(pick(6));
}
