//SPEC: §20.2 the construction expression coerces to the variant's payload type (T widens to ?T)
//OUT: 42
//OUT: -7
//OUT: 5

const MaybeBox = union(enum) {
    val: ?i64,
    tag: i64,
};

fn get(m: MaybeBox) i64 {
    switch (m) {
        .val => |o| {
            // the capture is the optional itself
            return o orelse 0 - 7;
        },
        .tag => |t| {
            return t;
        },
    }
}

pub fn main() void {
    // A bare i64 expression widens to the `?i64` payload at construction...
    print(get(MaybeBox{ .val = 21 * 2 }));      // 42
    // ...and `null` coerces too.
    print(get(MaybeBox{ .val = null }));        // orelse -> -7
    print(get(MaybeBox{ .tag = 5 }));           // 5
}
