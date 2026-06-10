//SPEC: §34.2 a `Set!T` whose set name is not a declared error set is E0331
//ERR: E0331

fn f() Bogus!i64 {
    return 3;
}

pub fn main() void {
    print(f() catch 0);
}
