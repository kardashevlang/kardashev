//SPEC: §9.4 field access requires a struct value — `.f` on an integer is E0165
//ERR: E0165
pub fn main() void {
    var n: i64 = 5;
    print(n.x);   // i64 has no fields
}
