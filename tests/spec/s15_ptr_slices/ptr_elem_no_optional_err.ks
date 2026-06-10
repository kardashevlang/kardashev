//SPEC: §15.1 a pointer's element is a plain type name — `*?T` does not parse (E0200)
//ERR: E0200

fn f(p: *?i32) void {
    print(0);
}

pub fn main() void {
    print(1);
}
