//SPEC: §36.1 a capture is in scope throughout its handler — including inside a NESTED catch, whose own capture coexists with it
//OUT: 1
//OUT: 1

fn fa() !i32 {
    return error.Alpha;
}

fn fb() !i32 {
    return error.Beta;
}

pub fn main() void {
    // Observe each code directly first (self-consistent: no absolute codes).
    var ca: i32 = fa() catch |e| e;
    var cb: i32 = fb() catch |e| e;
    // Outer handler = a nested catch. Inside the INNER handler both `e`
    // (outer, Alpha) and `e2` (inner, Beta) are visible.
    var got: i32 = fa() catch |e| (fb() catch |e2| e * 100 + e2 * 10 + e);
    if (got == ca * 100 + cb * 10 + ca) {
        print(1);
    } else {
        print(0);
    }
    // The two distinct error names captured distinct codes.
    if (ca == cb) {
        print(0);
    } else {
        print(1);
    }
}
