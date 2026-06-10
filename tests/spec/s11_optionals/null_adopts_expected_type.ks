//SPEC: §11.1 `null` takes its `?T` type from the expected type at every documented position
//OUT: -1
//OUT: -1
//OUT: -2
//OUT: -3
//OUT: -4
//OUT: 8

const Box = struct {
    v: ?i64,
};

fn give(flag: bool) ?i64 {
    if (flag) {
        return 8;
    }
    return null;                       // return position
}

fn reading(v: ?i64) i64 {
    return v orelse 0 - 2;
}

pub fn main() void {
    var a: ?i64 = null;                // initializer position
    print(a orelse 0 - 1);             // -1
    a = 5;
    a = null;                          // assignment position
    print(a orelse 0 - 1);             // -1
    print(reading(null));              // call-argument position -> -2
    var b: Box = Box{ .v = null };     // struct field-init position
    print(b.v orelse 0 - 3);           // -3
    print(give(false) orelse 0 - 4);   // return position -> -4
    print(give(true) orelse 0 - 4);    // 8 (the non-null path still works)
}
