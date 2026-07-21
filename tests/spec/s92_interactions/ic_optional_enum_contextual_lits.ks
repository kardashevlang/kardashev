//SPEC: §13 x §11 contextual `.V` literals flow through optional widening and as the orelse alternative
//OUT: 1
//OUT: 2

const Color = enum { Red, Green, Blue };

fn code(c: Color) i64 {
    switch (c) {
        .Red => { return 0; },
        .Green => { return 1; },
        .Blue => { return 2; },
    }
}

pub fn main() void {
    var oc: ?Color = .Green;          // the contextual literal widens into ?Color
    print(code(oc orelse .Blue));     // has a value → Green
    var none: ?Color = null;
    print(code(none orelse .Blue));   // null → the contextual alternative
}
