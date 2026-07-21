//SPEC: §23 x §20 a `[]u8` union payload: the capture re-slices and indexes the same backing bytes
//OUT: 9
//OUT: dash
//OUT: 100

const S = union(enum) { text: []u8, none: bool };

pub fn main() void {
    var s: S = S{ .text = "kardashev" };
    switch (s) {
        .text => |t| {
            print(t.len);      // 9
            print(t[3..7]);    // "dash"
            print(t[3]);       // 'd' is byte 100
        },
        .none => { print(0); },
    }
}
