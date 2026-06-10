//SPEC: §13.2 a `switch` covering every variant of its enum scrutinee is exhaustive without `else`
//OUT: 10
//OUT: 2
//OUT: 24
//OUT: 5

// Each variant dispatches to a different arithmetic op; if any arm were
// skipped or misrouted, the printed results would change.
const Op = enum { Add, Sub, Mul, Div };

fn apply(op: Op, a: i64, b: i64) i64 {
    switch (op) {
        .Add => { return a + b; },
        .Sub => { return a - b; },
        .Mul => { return a * b; },
        .Div => { return a / b; },
    }
}

pub fn main() void {
    print(apply(.Add, 6, 4));
    print(apply(.Sub, 6, 4));
    print(apply(.Mul, 6, 4));
    print(apply(.Div, 20, 4));
}
