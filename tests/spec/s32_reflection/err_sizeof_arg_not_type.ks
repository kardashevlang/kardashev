//SPEC: §32.1 the builtin's single argument must NAME a type (an Ident, resolved like `alloc`'s type argument §16) — a literal is rejected
//ERR: E0241
pub fn main() void {
    print(@sizeOf(42));
}
