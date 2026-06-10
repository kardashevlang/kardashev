//SPEC: §3 every value identifier and every callee must resolve to something in scope
//ERR: E0100
// Two halves of the same name-resolution bullet: an unknown value name and an
// unknown callee; each independently produces E0100.
pub fn main() void {
    print(zork);
    blat(1);
}
