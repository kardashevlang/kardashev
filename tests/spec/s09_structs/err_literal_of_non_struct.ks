//SPEC: §9.4 a struct literal whose name is not a struct is E0163
//ERR: E0163
// `answer` is an ordinary const value binding (§2), not a struct declaration,
// so `answer{ ... }` cannot construct anything.
const answer = 42;

pub fn main() void {
    var p = answer{ .x = 1 };
}
