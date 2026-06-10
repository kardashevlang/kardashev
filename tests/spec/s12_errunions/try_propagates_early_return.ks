//SPEC: §12.1 `try` yields the payload on success; on error it returns from the enclosing function, skipping the rest
//OUT: 20
//OUT: 40
//OUT: 6
//OUT: 30
//OUT: -1

// Each successful step prints a witness, so the output shows exactly how far
// execution got before a `try` propagated.
fn effect(n: i64) !i64 {
    if (n == 0) {
        return error.Zero;
    }
    print(n * 10);
    return n;
}

fn run_ok() !i64 {
    var a: i64 = try effect(2);       // prints 20, a = 2
    var b: i64 = try effect(a + 2);   // prints 40, b = 4
    return a + b;                     // 6
}

fn run_fails() !i64 {
    var a: i64 = try effect(3);       // prints 30, a = 3
    var b: i64 = try effect(a - 3);   // effect(0): propagates error.Zero
    var c: i64 = try effect(5);       // must never run (no 50 on stdout)
    return a + b + c;
}

pub fn main() void {
    print(run_ok() catch 0 - 1);      // 6
    print(run_fails() catch 0 - 1);   // -1; only "30" printed inside
}
