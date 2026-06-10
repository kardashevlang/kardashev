//SPEC: §11.2 a `T` value widens to `?T` at a struct field-init whose field is `?T`
//OUT: 60
//OUT: 2

const Reading = struct {
    id: i64,
    value: ?i64,
};

// Every third reading is missing; the others carry a plain i64 that widens to
// the `?i64` field at the struct-literal field-init site.
fn measure(i: i64) Reading {
    if (i % 3 == 0) {
        return Reading{ .id = i, .value = null };
    }
    return Reading{ .id = i, .value = i * 5 };   // plain i64 -> ?i64 field
}

pub fn main() void {
    var total: i64 = 0;
    var missing: i64 = 0;
    var i: i64 = 1;
    while (i <= 6) : (i = i + 1) {
        var r: Reading = measure(i);
        var v: i64 = r.value orelse 0 - 1;
        if (v == 0 - 1) {
            missing = missing + 1;
        } else {
            total = total + v;
        }
    }
    print(total);     // 5 + 10 + 20 + 25 = 60 (i = 3, 6 missing)
    print(missing);   // 2
}
