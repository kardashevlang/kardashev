//SPEC: §3 a type annotation must name a builtin or declared type
//ERR: E0100
pub fn main() void {
    var x: i65 = 0;
    print(x);
}
