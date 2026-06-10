//SPEC: §11.3×§12.3 an `orelse` RHS containing a `catch` is evaluated eagerly (documented v0.114 eagerness) — the catch swallows its error and never disturbs the orelse result
//OUT: 900
//OUT: 9
//OUT: 900
//OUT: -7

fn boom() !i64 {
    print(900);          // side effect proves the RHS ran
    return error.Boom;
}

fn mk_opt(n: i64) ?i64 {
    if (n == 0) {
        return null;
    }
    return n + 5;
}

pub fn main() void {
    // LHS has a value: the eager RHS still runs (900 prints), but its
    // result — boom()'s error caught to -7 — is discarded; a = 9.
    var a: i64 = mk_opt(4) orelse (boom() catch 0 - 7);
    print(a);

    // LHS null: the same chain's value is used; the inner catch turns the
    // error into -7 and the orelse yields it.
    var b: i64 = mk_opt(0) orelse (boom() catch 0 - 7);
    print(b);
}
