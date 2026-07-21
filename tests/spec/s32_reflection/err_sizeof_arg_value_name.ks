//SPEC: §32.1 `@sizeOf`'s argument must NAME a type — an identifier naming a runtime value is rejected
//ERR: E0321

pub fn main() void {
    var x: i64 = 4;
    print(@sizeOf(x));
}
