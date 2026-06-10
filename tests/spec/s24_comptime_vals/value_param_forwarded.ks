//SPEC: §24.2 a bound value parameter joins the constant environment — a generic body can forward it (with arithmetic) into a nested generic call
//OUT: 32
//OUT: 65

// Inside `shifted`'s instance the bound `n` is a known constant, so `n + 1`
// const-evaluates as the value argument of the nested `mul` call —
// instantiating mul at 3 and 6 from shifted at 2 and 5. (Declaration order is
// deliberately callee-after-caller: instantiation is demand-driven.)
fn shifted(comptime n: i64, x: i64) i64 {
    return mul(n + 1, x) + n;
}

fn mul(comptime m: i64, x: i64) i64 {
    return m * x;
}

pub fn main() void {
    print(shifted(2, 10)); // mul(3,10) + 2 = 32
    print(shifted(5, 10)); // mul(6,10) + 5 = 65
}
