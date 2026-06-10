//SPEC: §40.1 a labeled `break` must name an ENCLOSING loop's label
//ERR: E0121

pub fn main() void {
    var i: i64 = 0;
    sibling: while (i < 1) : (i = i + 1) {
    }
    while (i < 3) : (i = i + 1) {
        break :sibling;   // `sibling` is not enclosing — already closed above
    }
}
