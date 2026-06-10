//SPEC: §39.1 a range label `lo..hi` is only valid in a `switch` over an integer type, not an enum
//ERR: E0212

const Color = enum { Red, Green, Blue };

pub fn main() void {
    var c: Color = .Red;
    switch (c) {
        0..2 => { print(0); },   // ranges never apply to enum scrutinees
        else => {},
    }
}
