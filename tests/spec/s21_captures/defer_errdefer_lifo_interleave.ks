//SPEC: §21.2 defers and errdefers interleave in one LIFO sequence on the error path
//OUT: 30
//OUT: 10
//OUT: 7
//OUT: 0
//OUT: 40
//OUT: 30
//OUT: 20
//OUT: 10
//OUT: -1

// Registration order: defer 10, errdefer 20, defer 30, errdefer 40.
//   success:        30, 10            (defers only, reverse)
//   error-return:   40, 30, 20, 10    (both kinds, merged, reverse)
fn run(fail: bool) !i64 {
    defer print(10);
    errdefer print(20);
    defer print(30);
    errdefer print(40);
    if (fail) {
        return error.Boom;
    }
    return 7;
}

pub fn main() void {
    print(run(false) catch 0 - 1);
    print(0);                           // separator between the two paths
    print(run(true) catch 0 - 1);
}
