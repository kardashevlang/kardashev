//SPEC: §13.2 a `switch` on an integer scrutinee requires an `else` arm
//ERR: E0214

pub fn main() void {
    var n: i64 = 1;
    switch (n) {           // integer switches can never be proven exhaustive
        0 => { print(0); },
        1 => { print(1); },
    }
}
