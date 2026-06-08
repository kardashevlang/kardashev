// enums.ks — enums + exhaustive `switch` (v0.116).
//
// A plain enum is a small set of named variants. `switch` must cover every
// variant of an enum (or use `else`); for integers an `else` is required.
// Enum values are `Enum.Variant` (qualified) or `.Variant` (inferred).

const Op = enum { Add, Sub, Mul, Div };

fn apply(op: Op, a: i32, b: i32) i32 {
    switch (op) {               // exhaustive over all four variants — no `else`
        .Add => { return a + b; },
        .Sub => { return a - b; },
        .Mul => { return a * b; },
        .Div => { return a / b; },
    }
}

// Classify an integer with an integer `switch` (multi-label arm + `else`).
fn sign(n: i32) i32 {
    switch (n) {
        0 => { return 0; },
        else => {
            if (n < 0) { return 0 - 1; }
            return 1;
        },
    }
}

pub fn main() i32 {
    print(apply(.Add, 6, 4));    // 10
    print(apply(Op.Mul, 6, 4));  // 24
    print(apply(.Div, 20, 5));   // 4
    print(sign(0));              // 0
    print(sign(0 - 8));          // -1
    print(sign(8));              // 1
    return 0;
}

test "apply and sign" {
    expect(apply(.Sub, 10, 3) == 7);
    expect(apply(.Mul, 7, 7) == 49);
    expect(sign(0) == 0);
    expect(sign(5) == 1);
}
