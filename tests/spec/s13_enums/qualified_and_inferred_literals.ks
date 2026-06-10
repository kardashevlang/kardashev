//SPEC: §13.1 enum values are written `Enum.V` (qualified) or `.V` (enum type taken from context)
//OUT: 20
//OUT: 30
//OUT: 10

const Color = enum { Red, Green, Blue };

fn code(c: Color) i64 {
    switch (c) {
        .Red => { return 10; },
        Color.Green => { return 20; },   // a qualified switch label
        .Blue => { return 30; },
    }
}

pub fn main() void {
    var c: Color = .Green;       // `.V` inferred from the annotated init
    print(code(c));
    print(code(Color.Blue));     // qualified expression form
    print(code(.Red));           // `.V` inferred from the parameter type
}
