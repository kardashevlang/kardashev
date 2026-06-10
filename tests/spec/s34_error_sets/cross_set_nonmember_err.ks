//SPEC: §34.2 membership is per-set: an error declared in ANOTHER set is still E0330 against this set's `Set!T`
//ERR: E0330

const AErr = error{ AOnly };
const BErr = error{ BOnly };

// AOnly is a declared error name (member of AErr) — but not of BErr.
fn f() BErr!i64 {
    return error.AOnly;
}

pub fn main() void {
    print(f() catch 0);
}
