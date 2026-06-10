//SPEC: §15.1 `&place` must not be rooted in a `const` binding's own storage — there are no const pointers, so the `*T` would mutate a `const` (E0233)
//ERR: E0233

pub fn main() void {
    const n: i64 = 5;
    var q: *i64 = &n; // error: address of a `const` binding
    q.* = 7;
    print(n);
}
