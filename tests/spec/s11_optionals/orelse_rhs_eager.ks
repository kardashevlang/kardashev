//SPEC: §11.3 the `orelse` alternative is evaluated eagerly, even when the left side is non-null
//OUT: 555
//OUT: 6
//OUT: 555
//OUT: 9

// The fallback prints a witness each time it is evaluated. Per §11.3 the
// lowering is a plain helper call, so the alternative's side effect happens
// on BOTH the non-null and the null path (555 appears twice).
fn fallback() i64 {
    print(555);
    return 9;
}

fn maybe(n: i64) ?i64 {
    if (n > 0) {
        return n * 3;
    }
    return null;
}

pub fn main() void {
    print(maybe(2) orelse fallback());       // 555 (eager), then 6
    print(maybe(0 - 1) orelse fallback());   // 555, then the fallback 9
}
