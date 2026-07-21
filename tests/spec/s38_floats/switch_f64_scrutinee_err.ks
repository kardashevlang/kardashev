//SPEC: §13.2 a `switch` scrutinee must be an enum, an integer, or a tagged union — f64 is rejected
//ERR: E0213

pub fn main() void {
    var x: f64 = 1.5;
    switch (x) {
        else => { print(0); },
    }
}
