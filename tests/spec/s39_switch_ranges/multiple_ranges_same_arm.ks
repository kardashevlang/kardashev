//SPEC: §39 `SwitchArm.ranges` is a Vec — one arm may carry SEVERAL ranges
//OUT: 1
//OUT: 1
//OUT: 0
//OUT: 1
//OUT: 1
//OUT: 0

fn hit(n: i64) i64 {
    switch (n) {
        1..2, 7..8 => { return 1; },
        else => { return 0; },
    }
}

pub fn main() void {
    print(hit(1));
    print(hit(2));
    print(hit(3));   // in the GAP between the two ranges -> else
    print(hit(7));
    print(hit(8));
    print(hit(9));
}
