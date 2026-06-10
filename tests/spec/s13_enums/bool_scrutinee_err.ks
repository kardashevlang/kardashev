//SPEC: §13.2 a `switch` scrutinee must be an enum or an integer type
//ERR: E0213

pub fn main() void {
    var b: bool = true;
    switch (b) {           // bool is not switchable
        else => { print(0); },
    }
}
