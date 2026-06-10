//SPEC: §13.2 a `switch` scrutinee may be an integer type; labels are integer constants, `else` takes the rest
//OUT: 322

// An i32 scrutinee: the integer labels adopt the scrutinee's type. Summing
// over i = 0..6 hits label 0 three times, label 1 twice, `else` twice:
// 3*100 + 2*10 + 2*1 = 322.
fn bucket(n: i32) i64 {
    switch (n % 3) {
        0 => { return 100; },
        1 => { return 10; },
        else => { return 1; },
    }
}

pub fn main() void {
    var total: i64 = 0;
    var i: i32 = 0;
    while (i < 7) : (i = i + 1) {
        total = total + bucket(i);
    }
    print(total);
}
