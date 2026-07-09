// hello.ks — the canonical first program.
//
//   kard run examples/hello.ks     # prints 45 then 999
//   kard test examples/hello.ks    # runs the test block below

const LIMIT: i32 = comptime (5 * 2);

fn sum_to(n: i32) i32 {
    var total: i32 = 0;
    var i: i32 = 0;
    while (i < n) : (i = i + 1) {
        total = total + i;
    }
    return total;
}

pub fn main() i32 {
    // `defer` runs at scope exit — after the line below — in LIFO order.
    defer print(999);
    print(sum_to(LIMIT));
    return 0;
}

test "sum_to adds 0..n" {
    expect(sum_to(5) == 10);
}
