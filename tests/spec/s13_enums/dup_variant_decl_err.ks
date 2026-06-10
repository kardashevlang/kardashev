//SPEC: §13.2 a duplicate variant name within one enum declaration is rejected
//ERR: E0211

const E = enum { Alpha, Beta, Alpha };

pub fn main() void {
    print(0);
}
