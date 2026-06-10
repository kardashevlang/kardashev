//SPEC: §13.2 a `switch` label naming a non-existent variant of the scrutinee enum is rejected
//ERR: E0212

const Color = enum { Red, Green };

pub fn main() void {
    var c: Color = .Red;
    switch (c) {
        .Red => { print(0); },
        .Purple => { print(1); },    // not a variant of Color
        else => {},
    }
}
