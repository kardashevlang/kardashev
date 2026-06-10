//SPEC: §18.2 an unqualified `.Variant` initializer cannot infer a type — annotation required
//ERR: E0260
const Color = enum { Red, Green };
pub fn main() void {
    var c = .Red;
    print(0);
}
