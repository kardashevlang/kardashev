//SPEC: §17.2 a type argument identifier that resolves to no concrete type is E0251
//ERR: E0251

fn id(comptime T: type, x: T) T {
    return x;
}

pub fn main() void {
    // `n` is a perfectly good identifier — but it names a runtime value, not
    // a type, so it cannot bind the comptime type parameter.
    var n: i64 = 5;
    print(id(n, 7));
}
