//SPEC: §34.2 a member repeated within one `error{…}` declaration is E0331
//ERR: E0331

const E = error{ A, A };

pub fn main() void {
    print(0);
}
