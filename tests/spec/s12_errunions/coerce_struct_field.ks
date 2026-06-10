//SPEC: §12.2 `T -> !T` and `error.X -> !T` coerce at a struct field of type `!T` (field-init and field assignment)
//OUT: 33
//OUT: -1
//OUT: 1
//OUT: 8

const Slot = struct {
    r: !i64,
};

pub fn main() void {
    var ok: Slot = Slot{ .r = 30 + 3 };        // T -> !T at field-init
    print(ok.r catch 0 - 1);                   // 33
    var bad: Slot = Slot{ .r = error.Halt };   // error.X -> !T at field-init
    print(bad.r catch 0 - 1);                  // -1
    ok.r = error.Halt;                         // error.X -> !T at a field place
    print(ok.r catch |e| @as(i64, e));         // 1 (the sole error name)
    ok.r = 8;                                  // T -> !T at a field place
    print(ok.r catch 0 - 1);                   // 8
}
