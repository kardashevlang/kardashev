//SPEC: §39.1 a range label is only valid for an integer scrutinee — a tagged-union `switch` rejects it
//ERR: E0212

const V = union(enum) {
    a: i64,
    b: i64,
};

pub fn main() void {
    var v: V = V{ .a = 3 };
    switch (v) {
        1..5 => { print(1); },   // E0212: ranges never apply to a union switch
        else => { print(0); },
    }
}
