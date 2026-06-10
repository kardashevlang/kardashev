//SPEC: §24.2 a comptime value parameter binds per call; the function is monomorphised per distinct value
//OUT: 3
//OUT: 30
//OUT: 300

// One generic body whose behaviour must differ per bound `n`: a reference to
// `n` in the body is a compile-time constant, and the constant drives BOTH
// the arithmetic and which branch the instance takes. If all values collapsed
// into one instance the three columns could not disagree.
fn scale(comptime n: i64, x: i64) i64 {
    if (n > 50) {
        return x * n * 3; // only the n=100 instance takes this path
    }
    return x * n;
}

pub fn main() void {
    print(scale(1, 3)); // 3*1          = 3
    print(scale(10, 3)); // 3*10         = 30
    print(scale(100, 1)); // 1*100*3      = 300 (the >50 branch)
}
