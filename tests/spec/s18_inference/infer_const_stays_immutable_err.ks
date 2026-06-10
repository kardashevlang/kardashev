//SPEC: §18.2 an inferred local `const` is still immutable
//ERR: E0110
pub fn main() void {
    const c = 41; // no annotation — still a const binding
    c = c + 1;
    print(c);
}
