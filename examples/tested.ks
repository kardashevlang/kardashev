// tested.ks — `kard test --filter` and `kard bench` (v0.150).
//
//   kard test  examples/tested.ks               # run every test
//   kard test  examples/tested.ks --filter math # only tests whose name has "math"
//   kard bench examples/tested.ks               # run them with per-test timing
//
// Each `test "name" { … }` block is a named test; `--filter` keeps the ones
// whose name contains the substring, and `bench` reports `<name>: <ms> ms`.

fn fib(n: i32) i32 {
    if (n < 2) {
        return n;
    }
    return fib(n - 1) + fib(n - 2);
}

fn factorial(n: i32) i32 {
    var acc: i32 = 1;
    var i: i32 = 2;
    while (i <= n) : (i += 1) {
        acc *= i;
    }
    return acc;
}

pub fn main() i32 {
    print(fib(10));        // 55
    print(factorial(5));   // 120
    return 0;
}

test "math: fib base cases" {
    expect(fib(0) == 0);
    expect(fib(1) == 1);
}

test "math: fib recurrence" {
    expect(fib(10) == 55);
    expect(fib(15) == 610);
}

test "math: factorial" {
    expect(factorial(0) == 1);
    expect(factorial(5) == 120);
}

test "string round-trip" {
    var s: []u8 = "kardashev";
    expect(s.len == 9);
    expect(s[0] == 107);
}
