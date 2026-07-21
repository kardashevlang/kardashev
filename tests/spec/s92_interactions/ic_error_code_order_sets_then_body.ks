//SPEC: §12.1 x §34 error codes are 1-based in intern order: named-set members (declaration order) precede body-order literals
//OUT: 1
//OUT: 2
//OUT: 3

const Early = error{ First, Second };

fn fail_first() Early!i64 { return error.First; }
fn fail_second() Early!i64 { return error.Second; }
fn fail_fresh() !i64 { return error.Fresh; }

pub fn main() void {
    print(fail_first() catch |e| @as(i64, e));    // First interned first: 1
    print(fail_second() catch |e| @as(i64, e));   // 2
    print(fail_fresh() catch |e| @as(i64, e));    // the first body-only literal: 3
}
