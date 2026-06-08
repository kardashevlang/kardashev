// fib.kd — recursion. Prints the first 10 Fibonacci numbers.

fn fib(n: i32) i32 {
    if (n < 2) {
        return n;
    }
    return fib(n - 1) + fib(n - 2);
}

pub fn main() i32 {
    var i: i32 = 0;
    while (i < 10) : (i = i + 1) {
        print(fib(i));
    }
    return 0;
}

test "fib base and step" {
    expect(fib(0) == 0);
    expect(fib(1) == 1);
    expect(fib(10) == 55);
}
