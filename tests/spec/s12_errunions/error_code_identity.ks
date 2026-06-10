//SPEC: §12.1 error names intern to one 1-based code space: a name has the same nonzero code everywhere; distinct names differ
//OUT: 1
//OUT: 0
//OUT: 1
//OUT: 1

fn a1() !i64 {
    return error.Shared;
}

fn a2() !i64 {
    return error.Shared;   // the SAME name, mentioned in a different function
}

fn a3() !i64 {
    return error.Other;
}

fn code(r: !i64) i64 {
    return r catch |e| @as(i64, e);
}

pub fn main() void {
    var c1: i64 = code(a1());
    var c2: i64 = code(a2());
    var c3: i64 = code(a3());
    if (c1 == c2) { print(1); } else { print(0); }   // same name -> same code
    if (c1 == c3) { print(1); } else { print(0); }   // distinct names -> distinct codes
    if (c1 > 0) { print(1); } else { print(0); }     // codes are 1-based (0 = "no error")
    if (c3 > 0) { print(1); } else { print(0); }
}
