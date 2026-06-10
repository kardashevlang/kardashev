//SPEC: §38 floats are runtime-only — a top-level `const` cannot fold a float literal
//ERR: E0130

const P: f64 = 3.14;

pub fn main() void {
    print(P);
}
