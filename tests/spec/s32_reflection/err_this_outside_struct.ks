//SPEC: §32.2 `@This()` denotes the ENCLOSING struct type — outside any struct it desugars to `Self`, which is unbound
//ERR: E0100
fn f() @This() {
    return 1;
}

pub fn main() void {
    print(f());
}
