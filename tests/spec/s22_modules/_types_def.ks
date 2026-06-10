// Import fixture (§22.1): type-defining items (a struct and an enum).
pub const Pair = struct {
    x: i64,
    y: i64,
};

pub const Gear = enum {
    Low,
    High,
};
