//SPEC: §11.1 `x.?` on a null optional panics: stderr message + exit code 101
//EXIT: 101
//OUT: 3

fn pos(n: i64) ?i64 {
    if (n > 0) {
        return n;
    }
    return null;
}

pub fn main() void {
    print(pos(3).?);        // present: prints 3 and continues
    print(pos(0 - 2).?);    // null: panics here with exit code 101
    print(888);             // never reached — must NOT appear on stdout
}
