//SPEC: §4.4 a deferred statement evaluates at FLUSH time against its own frame's locals — mutations after registration (and after the recursive call) are seen
//OUT: 1
//OUT: 200
//OUT: 300
//OUT: 6

fn fact(n: i64) i64 {
    var acc: i64 = n;
    defer print(acc);          // reads THIS frame's acc when the frame exits
    if (n <= 1) {
        return 1;              // acc still 1 here — the *100 below never ran
    }
    var sub: i64 = fact(n - 1);
    acc = acc * 100;           // mutated after the defer AND after recursion
    return n * sub;
}

pub fn main() void {
    // fact(1) flushes acc=1; fact(2) flushes 200; fact(3) flushes 300.
    print(fact(3));            // 6
}
