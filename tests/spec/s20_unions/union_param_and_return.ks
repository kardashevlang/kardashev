//SPEC: §20.2 a construction's result is `Type::Union(id)` — usable as a return value and parameter
//OUT: 70
//OUT: 1004

const Res = union(enum) {
    ok: i64,
    bad: i64,
};

// Returns a union: which variant comes back depends on a runtime branch.
fn classify(x: i64) Res {
    if (x > 0) {
        return Res{ .ok = x * 10 };
    }
    return Res{ .bad = 0 - x };
}

// Takes a union parameter and dispatches on it.
fn settle(r: Res, dflt: i64) i64 {
    switch (r) {
        .ok => |v| {
            return v;
        },
        .bad => |e| {
            return dflt + e;
        },
    }
}

pub fn main() void {
    print(settle(classify(7), 0));          // ok 70
    print(settle(classify(0 - 4), 1000));   // bad 4 -> 1004
}
