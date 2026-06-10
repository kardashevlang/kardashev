//SPEC: §39.1 a backwards range (`hi < lo`) is NOT an error — it simply matches nothing
//OUT: 0
//OUT: 0
//OUT: 0

fn f(n: i64) i64 {
    switch (n) {
        5..1 => { return 1; },   // empty: even 5, 3 and 1 fall to else
        else => { return 0; },
    }
}

pub fn main() void {
    print(f(1));
    print(f(3));
    print(f(5));
}
