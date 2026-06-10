//SPEC: §9.5 struct params and returns pass by value — the callee works on a copy
//OUT: 7
//OUT: 107
//OUT: 8
//OUT: 7
const Cnt = struct {
    n: i32,
};

fn bumped(c: Cnt, by: i32) Cnt {
    // Params are immutable; copy into a local, mutate the copy, return it.
    var local: Cnt = c;
    local.n = local.n + by;
    return local;
}

pub fn main() void {
    var a: Cnt = Cnt{ .n = 7 };
    var b: Cnt = bumped(a, 100);
    print(a.n);    // 7 — the callee mutated only its copy
    print(b.n);    // 107 — the returned value is a fresh struct
    var c: Cnt = bumped(a, 1);
    print(c.n);    // 8 — each call copies `a` afresh
    print(a.n);    // 7 — still untouched after two calls
}
