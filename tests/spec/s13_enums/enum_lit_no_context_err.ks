//SPEC: §13.2 an unqualified `.V` with no expected enum type at its position is rejected
//ERR: E0215

const Color = enum { Red, Green };

pub fn main() void {
    var x: i64 = .Green;   // the expected type here is i64, not an enum
    print(x);
}
