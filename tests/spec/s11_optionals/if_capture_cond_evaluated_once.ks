//SPEC: §21.1 the captured `if` condition is evaluated exactly once (into a temp)
//OUT: 7005
//OUT: 5
//OUT: 6997
//OUT: -1

// The producer prints a witness per evaluation. Each `if (src(..)) |v|` must
// print that witness exactly once — a re-evaluating lowering would print it
// twice (once for the test, once for the unwrap).
fn src(n: i64) ?i64 {
    print(7000 + n);
    if (n > 0) {
        return n;
    }
    return null;
}

pub fn main() void {
    if (src(5)) |v| {
        print(v);
    } else {
        print(0 - 1);
    }
    if (src(0 - 3)) |v| {
        print(v);
    } else {
        print(0 - 1);
    }
}
