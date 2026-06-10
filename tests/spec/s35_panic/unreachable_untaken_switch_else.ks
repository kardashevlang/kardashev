//SPEC: §35.1 `unreachable` in a switch `else` arm type-checks and has no effect while the arm is never taken
//OUT: 2
//OUT: 1

const Color = enum { Red, Green, Blue };

fn pick(c: Color) i64 {
    switch (c) {
        .Red => { return 1; },
        .Green => { return 2; },
        else => { unreachable; },   // Blue never passed in this program
    }
}

pub fn main() void {
    print(pick(.Green));
    print(pick(.Red));
}
