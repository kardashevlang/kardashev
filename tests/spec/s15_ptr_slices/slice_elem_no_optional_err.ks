//SPEC: §15.2 a slice's element is a plain type name — `[]?T` does not parse (E0200)
//ERR: E0200

fn f(s: []?i32) void {
    print(0);
}

pub fn main() void {
    print(1);
}
