//SPEC: §39 a range bound is an integer LITERAL — a negative bound does not parse
//ERR: E0200

pub fn main() void {
    var x: i64 = 0;
    switch (x) {
        -3..3 => { print(1); },
        else => { print(0); },
    }
}
