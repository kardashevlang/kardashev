//SPEC: §20.2 a `?i64` union payload: construction widens a bare i64 and accepts null; the capture feeds orelse
//OUT: 5
//OUT: -1

const Cell = union(enum) { v: ?i64 };

fn get(c: Cell) i64 {
    switch (c) {
        .v => |ov| { return ov orelse (0 - 1); },
    }
}

pub fn main() void {
    print(get(Cell{ .v = 5 }));      // payload widened i64 → ?i64
    print(get(Cell{ .v = null }));   // null adopts the payload type
}
