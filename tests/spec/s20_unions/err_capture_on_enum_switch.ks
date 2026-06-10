//SPEC: §20.2 a payload capture on a non-union `switch` (a plain enum) is E0272
//ERR: E0272

const Color = enum { Red, Green };

pub fn main() void {
    var c: Color = Color.Red;
    switch (c) {
        // A plain enum variant has no payload to capture.
        .Red => |x| {
            print(x);
        },
        .Green => {
            print(2);
        },
    }
}
