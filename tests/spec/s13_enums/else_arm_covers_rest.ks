//SPEC: §13.2 an `else` arm makes an enum `switch` exhaustive; it runs for every uncovered variant
//OUT: 3
//OUT: 300

const Tok = enum { Plus, Minus, Star, Slash, Percent };

fn weight(t: Tok) i64 {
    var w: i64 = 0;
    switch (t) {
        .Plus => { w = 1; },
        .Minus => { w = 2; },
        else => { w = 100; },
    }
    return w;
}

pub fn main() void {
    // The two explicitly covered variants take their own arms...
    print(weight(.Plus) + weight(.Minus));
    // ...and each of the three uncovered ones lands in `else`.
    print(weight(.Star) + weight(.Slash) + weight(.Percent));
}
