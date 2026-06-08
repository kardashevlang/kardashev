// optional.ks — optionals (v0.114): `?T`, `null`, `orelse`, `.?`.
//
// `?T` makes "maybe absent" explicit. A plain `T` coerces to `?T`; `null` is
// the empty value; `x orelse default` unwraps-or-defaults; `x.?` force-unwraps
// (and panics if null).

const Found = struct {
    index: ?i32,   // an optional struct field
};

// Linear search returning an optional index.
fn find(target: i32) ?i32 {
    var i: i32 = 0;
    while (i < 5) : (i = i + 1) {
        if (i * i == target) {
            return i;       // T coerces to ?T
        }
    }
    return null;
}

pub fn main() i32 {
    print(find(9) orelse 0 - 1);     // 3   (3*3 == 9)
    print(find(7) orelse 0 - 1);     // -1  (no i with i*i == 7)
    print(find(16).?);               // 4   (force-unwrap a known-present value)

    var f: Found = Found{ .index = find(4) };
    print(f.index orelse 0 - 1);     // 2

    return 0;
}

test "find optional" {
    expect((find(9) orelse 0 - 1) == 3);
    expect((find(7) orelse 100) == 100);
    expect(find(0).? == 0);
}
