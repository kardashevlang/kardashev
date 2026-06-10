//SPEC: §34.2 returning an `error.X` that is not a member of the function's named set `Set!T` is E0330 (cross-ref: §12 global `!T` accepts any error)
//ERR: E0330

const SmallErr = error{ A, B };

fn f(n: i64) SmallErr!i64 {
    if (n == 0) {
        return error.NotMember;   // not in SmallErr
    }
    return n;
}

pub fn main() void {
    print(f(1) catch 0);
}
