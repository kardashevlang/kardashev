// collatz.kd — control flow: the Collatz sequence starting at 27.
//
// Demonstrates `while (cond) : (cont)`, `if/else`, and `break`.

pub fn main() i32 {
    var n: i32 = 27;
    var steps: i32 = 0;
    while (n != 1) : (steps = steps + 1) {
        print(n);
        if (n > 1000000) {
            break; // guard against overflow on pathological inputs
        }
        if (n % 2 == 0) {
            n = n / 2;
        } else {
            n = 3 * n + 1;
        }
    }
    print(n);     // 1
    print(steps); // 111 for a start of 27
    return 0;
}

test "collatz of 27 takes 111 steps" {
    var n: i32 = 27;
    var steps: i32 = 0;
    while (n != 1) : (steps = steps + 1) {
        if (n % 2 == 0) {
            n = n / 2;
        } else {
            n = 3 * n + 1;
        }
    }
    expect(steps == 111);
}
