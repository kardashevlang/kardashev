//SPEC: §15.2 `a[k..k]` is a legal empty slice (lo == hi), including at both ends of the array
//OUT: 0
//OUT: 0
//OUT: 100

pub fn main() void {
    var data: [4]i64 = [4]i64{ 1, 2, 3, 4 };

    var front: []i64 = data[0..0];
    print(front.len);

    var back: []i64 = data[4..4]; // lo == hi == len: still in bounds
    print(back.len);

    // Iterating an empty view runs zero times — the accumulator is untouched.
    var mid: []i64 = data[2..2];
    var sum: i64 = 100;
    var i: usize = 0;
    while (i < mid.len) : (i += 1) {
        sum = sum + mid[i];
    }
    print(sum);
}
