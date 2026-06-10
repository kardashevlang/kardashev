//SPEC: §28.1 equality binds tighter than `&` — `1 & 2 == 2` is 1 & (2 == 2), an int-vs-bool type error; were `&` tighter it would compile
//ERR: E0110
pub fn main() void {
    var x: i64 = 1 & 2 == 2;
    print(x);
}
