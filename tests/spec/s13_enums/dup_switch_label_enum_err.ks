//SPEC: §13.2 the same enum variant may not label two `switch` arms
//ERR: E0211

const Color = enum { Red, Green, Blue };

pub fn main() void {
    var c: Color = .Red;
    switch (c) {
        .Red => { print(1); },
        .Green => { print(2); },
        .Red => { print(3); },     // duplicate label
        else => {},
    }
}
