//SPEC: §2 return_stmt := "return" expr? ";" — value returns, early returns, and the bare `return;` in a void function
//OUT: 42
//OUT: 4
//OUT: 9
fn pick(n: i64) i64 {
    if (n > 0) {
        return n * 2; // early return skips the fallback below
    }
    return 0 - n;
}

fn log_nonzero(n: i64) void {
    if (n == 0) {
        return; // bare return: leaves without printing
    }
    print(n);
}

pub fn main() void {
    print(pick(21));
    print(pick(0 - 4));
    log_nonzero(0);
    log_nonzero(9);
}
