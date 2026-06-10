//SPEC: §36.1 an inner catch capture may reuse the outer's name — the inner binding shadows it inside the inner handler
//OUT: 1
//OUT: 1

fn fa() !i32 {
    return error.Alpha;
}

fn fb() !i32 {
    return error.Beta;
}

pub fn main() void {
    var ca: i32 = fa() catch |e| e;
    var cb: i32 = fb() catch |e| e;
    // Both captures are named `e`; inside the inner handler `e` must be the
    // INNER (Beta) code, not the outer (Alpha) one.
    var sh: i32 = fa() catch |e| (fb() catch |e| e);
    if (sh == cb) {
        print(1);
    } else {
        print(0);
    }
    if (sh == ca) {
        print(0);
    } else {
        print(1);
    }
}
