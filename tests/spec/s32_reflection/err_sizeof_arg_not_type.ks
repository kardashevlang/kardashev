//SPEC: §32.1 the builtin's single argument must NAME a type — a literal is rejected
//ERR: E0321

pub fn main() void {
    print(@sizeOf(42));
}
