//SPEC: §34.2 set members live in the one global error-code space: the same name in two sets and in a global `!T` is the SAME error value
//OUT: 1

const AErr = error{ Shared, AOnly };
const BErr = error{ Shared, BOnly };

fn fa() AErr!i64 {
    return error.Shared;
}

fn fb() BErr!i64 {
    return error.Shared;
}

// The same name with no set constraint at all (global `!T`).
fn fg() !i64 {
    return error.Shared;
}

pub fn main() void {
    var ca: i64 = fa() catch |e| @as(i64, e);
    var cb: i64 = fb() catch |e| @as(i64, e);
    var cg: i64 = fg() catch |e| @as(i64, e);
    // `error.Shared` keeps one stable code regardless of which set typed it.
    if (ca == cb and cb == cg) {
        print(1);
    } else {
        print(0);
    }
}
