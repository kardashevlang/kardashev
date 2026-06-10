//SPEC: §14.1+§23 an array element type is a NAMED type — `[2][]u8` (an array of strings) is not expressible, E0200
//ERR: E0200

pub fn main() void {
    var a: [2][]u8 = [2][]u8{ "hi", "yo" };
    print(a[0]);
}
