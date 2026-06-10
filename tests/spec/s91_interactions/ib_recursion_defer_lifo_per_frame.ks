//SPEC: §4.4 defers are per-FRAME under recursion — each call flushes its own defers (LIFO within the frame) when it returns, deepest frame first
//OUT: 0
//OUT: 0
//OUT: 100
//OUT: 1
//OUT: 200
//OUT: 2
//OUT: 300
//OUT: 3
//OUT: 6

// Frame n registers print(n) then print(n*100); LIFO flips them to
// n*100, n at that frame's return. Frames return inner-first: 0, 1, 2, 3.
fn countdown(n: i64) i64 {
    defer print(n);
    defer print(n * 100);
    if (n == 0) {
        return 0;
    }
    var rest: i64 = countdown(n - 1);
    return rest + n;
}

pub fn main() void {
    print(countdown(3));    // 0+1+2+3 = 6, after all frames' defers
}
