//SPEC: §21.2 a success `return` flushes only `defer`s (reverse order) — `errdefer`s do not fire
//OUT: 3
//OUT: 1
//OUT: 42

fn compute(flag: bool) !i64 {
    defer print(1);             // registered first
    errdefer print(2);          // must NOT appear on this path
    defer print(3);             // registered last -> flushes first
    if (flag) {
        return error.Nope;
    }
    var acc: i64 = 40;
    acc = acc + 2;
    return acc;                 // success return: 3 then 1, no 2
}

pub fn main() void {
    print(compute(false) catch 0 - 1);  // defers print before the caller sees 42
}
