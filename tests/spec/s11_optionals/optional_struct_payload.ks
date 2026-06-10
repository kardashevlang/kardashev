//SPEC: §11 `?T` where `T` is a struct: null/payload round-trip, `.?` member access, captured payload
//OUT: 16
//OUT: 56
//OUT: -1
//OUT: 6

const P = struct {
    x: i64,
    y: i64,
};

fn mk(n: i64) ?P {
    if (n == 0) {
        return null;
    }
    return P{ .x = n, .y = n * n };   // a struct value widens to ?P
}

pub fn main() void {
    print(mk(4).?.y);                  // unwrap then member access: 16
    if (mk(7)) |p| {
        print(p.x + p.y);              // captured struct payload: 7 + 49 = 56
    } else {
        print(0 - 1);
    }
    if (mk(0)) |p| {
        print(p.x);
    } else {
        print(0 - 1);                  // null struct optional: -1
    }
    var slot: ?P = null;
    slot = P{ .x = 2, .y = 3 };        // struct widens on assignment too
    print(slot.?.x * slot.?.y);        // 6
}
