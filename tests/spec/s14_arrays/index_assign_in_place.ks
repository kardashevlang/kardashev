//SPEC: §14.2 `a[i] = e` writes one element of a mutable `var` array in place
//OUT: 54321

// Reverse {1,2,3,4,5} in place by swapping pairs, then read the digits back
// out as one number: 54321. A missed or misplaced write changes the digits.
pub fn main() void {
    var a: [5]i64 = [5]i64{ 1, 2, 3, 4, 5 };
    var i: usize = 0;
    while (i < 2) : (i = i + 1) {
        var tmp: i64 = a[i];
        a[i] = a[4 - i];
        a[4 - i] = tmp;
    }
    var out: i64 = 0;
    var j: usize = 0;
    while (j < 5) : (j = j + 1) {
        out = out * 10 + a[j];
    }
    print(out);
}
