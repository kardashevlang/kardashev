//SPEC: §15.1 `&place` accepts an array index as the lvalue — writes through it land in the array
//OUT: 55

// Square every element strictly through element pointers, then sum by reading
// the array directly: 1 + 4 + 9 + 16 + 25 = 55.
pub fn main() void {
    var a: [5]i64 = [5]i64{ 1, 2, 3, 4, 5 };
    var i: usize = 0;
    while (i < 5) : (i += 1) {
        var p: *i64 = &a[i];
        p.* = p.* * p.*;
    }

    var sum: i64 = 0;
    i = 0;
    while (i < 5) : (i += 1) {
        sum = sum + a[i];
    }
    print(sum);
}
