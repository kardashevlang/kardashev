//SPEC: §39 value labels and ranges combine in ONE arm — it matches any label OR any range
//OUT: 1
//OUT: 1
//OUT: 1
//OUT: 1
//OUT: 0
//OUT: 0

fn hit(n: i64) i64 {
    switch (n) {
        5, 10..12 => { return 1; },
        else => { return 0; },
    }
}

pub fn main() void {
    print(hit(5));   // the lone value label
    print(hit(10));  // range lo
    print(hit(11));  // range interior
    print(hit(12));  // range hi
    print(hit(6));   // between label and range -> else
    print(hit(13));  // past the range -> else
}
