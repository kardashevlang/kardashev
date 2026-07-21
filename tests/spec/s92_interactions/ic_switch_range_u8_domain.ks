//SPEC: §39 x §28.4 a u8 scrutinee: range arms classify the byte domain, both endpoints inclusive
//OUT: 1
//OUT: 2
//OUT: 3

fn class(b: u8) i64 {
    switch (b) {
        0..99 => { return 1; },
        100..199 => { return 2; },
        else => { return 3; },
    }
}

pub fn main() void {
    var x: u8 = 99;
    print(class(x));
    var y: u8 = 100;
    print(class(y));
    var z: u8 = 255;
    print(class(z));
}
