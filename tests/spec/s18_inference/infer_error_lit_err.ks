//SPEC: §18.2 an `error.X` initializer cannot infer a type — annotation required
//ERR: E0260
pub fn main() void {
    var e = error.Boom;
    print(0);
}
