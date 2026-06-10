//SPEC: §35.1 a diverging `unreachable` adopts an annotated init type (`var x: i32 = unreachable;` type-checks) and exits 101 when evaluated
//EXIT: 101
//OUT: 1

pub fn main() void {
    print(1);
    var x: i32 = unreachable;   // adopts i32; evaluating it diverges
    print(x);                   // never reached
}
