//SPEC: §18.2 a bare `null` initializer cannot infer a type — annotation required
//ERR: E0260
pub fn main() void {
    var x = null;
    print(0);
}
