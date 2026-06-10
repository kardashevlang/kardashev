//SPEC: §24.2 an `[n]T` whose `n` is not a comptime value parameter in scope is E0253
//ERR: E0253

// `first` is NOT generic — `k` names nothing, so the array size cannot
// resolve (the §24.1 `ArraySize::Param` form is only meaningful inside a
// generic that binds it).
fn first(xs: [k]i64) i64 {
    return xs[0];
}

pub fn main() void {
    print(first([2]i64{ 1, 2 }));
}
